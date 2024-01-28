#!/usr/bin/env bash
# spell-checker:ignore termux keyevent sdcard binutils unmatch adb's dumpsys logcat pkill nextest logfile

# There are three shells: the host's, adb, and termux. Only adb lets us run
# commands directly on the emulated device, only termux provides a GNU
# environment on the emulated device (to e.g. run cargo). So we use adb to
# launch termux, then to send keystrokes to it while it's running.
# This means that the commands sent to termux are first parsed as arguments in
# this shell, then as arguments in the adb shell, before finally being used as
# text inputs to the app. Hence, the "'wrapping'" on those commands.
# There's no way to get any direct feedback from termux, so every time we run a
# command on it, we make sure it creates a unique *.probe file which is polled
# every 30 seconds together with the current output of the command in a *.log file.
# The contents of the probe file are used as a return code: 0 on success, some
# other number for errors (an empty file is basically the same as 0). Note that
# the return codes are text, not raw bytes.

this_repo="$(dirname "$(dirname -- "$(readlink -- "${0}")")")"
cache_dir_name="__rust_cache__"
#dev_probe_dir=/data/data/com.termux/files/tmp
dev_probe_dir=/sdcard
dev_home_dir=/data/data/com.termux/files/home

# choose only reliable mirrors here:
repo_url_list=(
    "deb https://packages-cf.termux.org/apt/termux-main/ stable main"
    "deb https://packages-cf.termux.dev/apt/termux-main/ stable main"
    "deb https://grimler.se/termux/termux-main stable main"
    "deb https://ftp.fau.de/termux/termux-main stable main"
)
number_repo_urls=${#repo_url_list[@]}
repo_url_round_robin=$RANDOM

move_to_next_repo_url() {
    repo_url_round_robin=$(((repo_url_round_robin + 1) % number_repo_urls))
    echo "next round robin repo_url: $repo_url_round_robin"
}
move_to_next_repo_url # first call needed for modulo

get_current_repo_url() {
    echo "${repo_url_list[$repo_url_round_robin]}"
}

echo "====== runner information ======"
echo "hostname: $(hostname)"
echo "uname -a: $(uname -a)"
echo "pwd: $(pwd)"
echo "\$*: $*"
echo "\$0: $0"
# shellcheck disable=SC2140
echo "\$(readlink -- "\$\{0\}"): $(readlink -- "${0}")"
echo "\$this_repo: $this_repo"
echo "readlink -f \$this_repo: $(readlink -f $this_repo)"
echo "====== ================== ======"

this_repo=$(readlink -f "$this_repo")

help() {
    echo \
        "Usage: $0 COMMAND [ARG]

where COMMAND is one of:
  init          download termux and initialize the emulator image
  snapshot APK  install APK and dependencies on an emulator to prep a snapshot
                (you can, but probably don't want to, run this for physical
                devices -- just set up termux and the dependencies yourself)
  sync_host [REPO]
                push the repo at REPO to the device, deleting and restoring all symlinks (locally)
                in the process; The cached rust directories are restored, too; by default, REPO is:
                $this_repo
  sync_image [REPO]
                copy the repo/target and the HOME/.cargo directories from the device back to the
                host; by default, REPO is: $this_repo
  build         run \`cargo build --features feat_os_unix_android\` on the
                device
  tests         run \`cargo test --features feat_os_unix_android\` on the
                device

If you have multiple devices, use the ANDROID_SERIAL environment variable to
specify which to connect to."
}

setup_tmp_dir() {
    adb shell input text \"cd\" && hit_enter
    adb shell input text \"mkdir ../tmp\" && hit_enter
    adb shell input text \"chmod a+rwx ../.. .. ../tmp\" && hit_enter
}

hit_enter() {
    adb shell input keyevent 66
}

exit_termux() {
    adb shell input text \"exit\" && hit_enter && hit_enter
}

launch_termux() {
    echo "launching termux"
    if ! adb shell 'am start -n com.termux/.HomeActivity'; then
        echo "failed to launch termux"
        exit 1
    fi
    # the emulator can sometimes be a little slow to launch the app
    while ! adb shell "ls $dev_probe_dir/launch.probe" 2>/dev/null; do
        echo "waiting for launch.probe"
        sleep 5
        setup_tmp_dir
        adb shell input text "\"touch $dev_probe_dir/launch.probe\"" && hit_enter
    done
    echo "found launch.probe"
    adb shell "rm $dev_probe_dir/launch.probe" && echo "removed launch.probe"
}

chmod_target_file() {
    adb shell input text "\"chmod a+rw $1\""  &&  hit_enter
}

# Usage: run_termux_command
#
# Runs the command specified in $1 in a termux shell, polling for the probe specified in $2 (and the
# current output). If polling the probe succeeded the command is considered to have finished. This
# method prints the current stdout and stderr of the command every SLEEP_INTERVAL seconds and
# finishes a command run with a summary. It returns with the exit code of the probe if specified as
# file content of the probe.
#
# Positional arguments
# $1                The command to execute in the termux shell
# $2                The path to the probe. The file name must end with `.probe`
#
# It's possible to overwrite settings by specifying the setting the variable before calling this
# method (Default in parentheses):
# keep_log  0|1     Keeps the logs after running the command if set to 1. The log file name is
#                   derived from the probe file name (the last component of the path) and
#                   `.probe` replaced with `.log. (0)
# debug     0|1     Adds additional debugging output to the log file if set to 1. (1)
# timeout   SECONDS The timeout in full SECONDS for the command to complete before giving up. (3600)
# retries   RETRIES The number of retries for trying to fix possible issues when we're not receiving
#                   any progress from the emulator. (3)
# sleep_interval
#           SECONDS The time interval in full SECONDS between polls for the probe and the current
#           output. (5)
run_termux_command() {
    # shellcheck disable=SC2155
    local command="$(echo "$1" | sed -E "s/^['](.*)[']$/\1/")" # text of the escaped command, including creating the probe!
    local probe="$2"                                           # unique file that indicates the command is complete
    local keep_log=${keep_log:-0}
    local debug=${debug:-1}

    log_name="$(basename -s .probe "${probe}").log" # probe name must have suffix .probe
    log_file="$dev_probe_dir/${log_name}"
    log_read="${log_name}.read"
    echo 0 >"${log_read}"
    if [[ $debug -eq 1 ]]; then
        shell_command="'set -x; { ${command}; } &> ${log_file}; set +x'"
    else
        shell_command="'{ ${command}; } &> ${log_file}'"
    fi

    launch_termux
    echo "Running command: ${command}"
    start=$(date +%s)
    adb shell input text "$shell_command" && sleep 3 && hit_enter
    # just for safety wait a little bit before polling for the probe and the log file
    sleep 5

    local timeout=${timeout:-3600}
    local retries=${retries:-10}
    local sleep_interval=${sleep_interval:-10}
    try_fix=3
    echo "run_termux_command with timeout=$timeout / retries=$retries / sleep_interval=$sleep_interval"
    while ! adb shell "ls $probe" 2>/dev/null; do
        echo -n "Waiting for $probe: "

        chmod_target_file "$log_file"
        chmod_target_file "$probe"

        if [[ -e "$log_name" ]]; then
            rm "$log_name"
        fi

        chmod_target_file "$log_file"
        chmod_target_file "$probe"

        adb pull "$log_file" . || try_fix=$((try_fix - 1))
        if [[ -e "$log_name" ]]; then
            tail -n +"$(<"$log_read")" "$log_name"
            echo
            wc -l <"${log_name}" | tr -d "[:space:]" >"$log_read"
        fi

        if [[ retries -le 0 ]]; then
            echo "Maximum retries reached running command. Aborting ..."
            return 1
        elif [[ try_fix -le 0 ]]; then
            retries=$((retries - 1))
            try_fix=3
            # Since there is no output, there is no way to know what is happening inside. See if
            # hitting the enter key solves the issue, sometimes the github runner is just a little
            # bit slow.
            echo "No output received. Trying to fix the issue ... (${retries} retries left)"
            hit_enter
        fi

        sleep "$sleep_interval"
        timeout=$((timeout - sleep_interval))

        if [[ $timeout -le 0 ]]; then
            echo "Timeout reached running command. Aborting ..."
            return 1
        fi
    done
    end=$(date +%s)

    chmod_target_file "$log_file"
    chmod_target_file "$probe"

    # exit 77

    return_code=$(adb shell "cat $probe") || return_code=0
    adb shell "rm ${probe}"

    # adb pull "$log_file" .
    adb shell "cat $log_file" > $log_name
    echo "==================================== SUMMARY ==================================="
    echo "Command: ${command}"
    echo "Finished in $((end - start)) seconds."
    echo "Output was:"
    cat "$log_name"
    echo "Return code: $return_code"
    echo "================================================================================"

    adb shell "rm ${log_file}"
    [[ $keep_log -ne 1 ]] && rm -f "$log_name"
    rm -f "$log_read" "$probe"

    # shellcheck disable=SC2086
    return $return_code
}

init() {
    arch="$1"
    # shellcheck disable=SC2034
    api_level="$2"
    termux="$3"

    snapshot_name="${AVD_CACHE_KEY}"

    # shellcheck disable=SC2015
    wget "https://github.com/termux/termux-app/releases/download/${termux}/termux-app_${termux}+github-debug_${arch}.apk" &&
        snapshot "termux-app_${termux}+github-debug_${arch}.apk" &&
        hash_rustc &&
        exit_termux &&
        adb -s emulator-5554 emu avd snapshot save "$snapshot_name" &&
        echo "Emulator image created. Name: $snapshot_name" || {
        pkill -9 qemu-system-x86_64
        return 1
    }
    pkill -9 qemu-system-x86_64 || true
}

reinit_ssh_connection() {
    setup_ssh_forwarding
    test_ssh_connection && return

    start_sshd_via_adb_shell && (
        test_ssh_connection && return
        generate_and_install_public_key && test_ssh_connection && return
    ) || (
        install_packages_via_adb_shell openssh openssl
        generate_and_install_public_key
        start_sshd_via_adb_shell
        test_ssh_connection && return
    ) || (
        echo "failed to setup ssh connection"
        return 1
    )
}

start_sshd_via_adb_shell() {
    echo "start sshd via adb shell"
    probe="$dev_probe_dir/sshd.probe"
    command="'sshd; echo \$? > $probe'"
    run_termux_command "$command" "$probe"
}

setup_ssh_forwarding() {
    echo "setup ssh forwarding"
    adb forward tcp:9022 tcp:8022
}

copy_file_or_dir_to_device_via_ssh() {
    scp -r "$1" "scp://termux@127.0.0.1:9022/$2"
}

copy_file_or_dir_from_device_via_ssh() {
    scp -r "scp://termux@127.0.0.1:9022/$1" "$2"
}

run_command_via_ssh() {
    ssh -p 9022 termux:@127.0.0.1 -o StrictHostKeyChecking=accept-new "$@"
}

test_ssh_connection() {
    run_command_via_ssh echo ssh connection is working
    run_command_via_ssh free -mh
}

run_script_file_via_ssh() {
    ssh -p 9022 termux:@127.0.0.1 -o StrictHostKeyChecking=accept-new "bash -s" < "$1"
}

navigate_down() {
    adb shell input keyevent 20
}

hit_space_key() {
    adb shell input text "\ "
}

termux_change_rep() {
    adb shell input text "termux-change-repo" && hit_enter
    sleep 1
    hit_enter  # select mirror group option
    sleep 1
    navigate_down
    navigate_down
    navigate_down # select europe
    hit_space_key
    hit_enter
}

adb_input_text_long() {
    string=$1
    length=${#string}
    step=20
    p=0
    for ((i = 0; i < length-step; i = i + step)); do
        chars="${string:i:$step}"
        adb shell input text "\"$chars\""
        p=$((i+step))
    done

    length=${#string}
    for ((i = p; i < length; i++)); do
        char="${string:i:1}"
        adb shell input text "\"$char\""
    done
}

generate_rsa_key_local() {
    yes "" | ssh-keygen -t rsa -b 4096 -C "Github Action" -N ""
}

install_rsa_pub() {

    run_command_via_ssh "echo hello" && return  # if this works, we are already fine. Skipping

    # remove old host identity:
    ssh-keygen -f ~/.ssh/known_hosts -R "[127.0.0.1]:9022"

    rsa_pub_key=$(cat ~/.ssh/id_rsa.pub)
    echo "====================================="
    echo "$rsa_pub_key"
    echo "====================================="

    adb shell input text \"echo \"

    adb_input_text_long "$rsa_pub_key"

    adb shell input text "\" >> ~/.ssh/authorized_keys\"" && hit_enter
    sleep 1
}

install_packages_via_adb_shell() {
    install_package_list="$*"

    install_packages_via_adb_shell_using_apt "$install_package_list"
    if [[ $? -ne 0 ]]; then
        echo "apt failed. Now try install with pkg as fallback."
        probe="$dev_probe_dir/pkg.probe"
        command="'mkdir -vp ~/.cargo/bin; yes | pkg install $install_package_list -y; echo \$? > $probe'"
        run_termux_command "$command" "$probe" || return 1
    fi

    return 0
}

install_packages_via_adb_shell_using_apt() {
    install_package_list="$*"

    repo_url=$(get_current_repo_url)
    move_to_next_repo_url
    echo "set apt repository url: $repo_url"
    probe="$dev_probe_dir/sourceslist.probe"
    command="'echo $repo_url | dd of=\$PREFIX/etc/apt/sources.list; echo \$? > $probe'"
    run_termux_command "$command" "$probe"

    probe="$dev_probe_dir/adb_install.probe"
    command="'mkdir -vp ~/.cargo/bin; apt update; yes | apt install $install_package_list -y; echo \$? > $probe'"
    run_termux_command "$command" "$probe"
}

install_packages_via_ssh_using_apt() {
    install_package_list="$*"

    repo_url=$(get_current_repo_url)
    move_to_next_repo_url
    echo "set apt repository url: $repo_url"
    run_command_via_ssh "echo $repo_url | dd of=\$PREFIX/etc/apt/sources.list"

    run_command_via_ssh "apt update; yes | apt install $install_package_list -y"
}

apt_upgrade_all_packages() {
    repo_url=$(get_current_repo_url)
    move_to_next_repo_url
    echo "set apt repository url: $repo_url"
    run_command_via_ssh "echo $repo_url | dd of=\$PREFIX/etc/apt/sources.list"

    run_command_via_ssh "apt update; yes | apt upgrade -y"
}


generate_and_install_public_key() {
    echo "generate local public private key pair"
    generate_rsa_key_local
    echo "install public key via 'adb shell input'"
    install_rsa_pub
    echo "installed ssh public key on device"
}

snapshot() {
    apk="$1"
    echo "Running snapshot"
    adb install -g "$apk"

    setup_tmp_dir
    echo "Prepare and install system packages"

    reinit_ssh_connection || return 1

    apt_upgrade_all_packages

    install_packages_via_ssh_using_apt "rust binutils openssl tar"

    echo "Installing cargo-nextest"
    # We need to install nextest via cargo currently, since there is no pre-built binary for android x86
    run_command_via_ssh "export CARGO_TERM_COLOR=always && cargo install cargo-nextest"
    return_code=$?

    echo "Info about cargo and rust - via SSH Script"
    run_script_file_via_ssh "$this_repo/util/android-scripts/collect-info.sh"

    echo "Info about cargo and rust"
    command="echo \$HOME; \
PATH=\$HOME/.cargo/bin:\$PATH; \
export PATH; \
echo \$PATH; \
pwd; \
command -v rustc && rustc -Vv; \
ls -la ~/.cargo/bin; \
cargo --list; \
cargo nextest --version"
    run_command_via_ssh "$command"

    echo "Snapshot complete"
    # shellcheck disable=SC2086
    return $return_code
}

sync_host() {
    repo="$1"
    cache_home="${HOME}/${cache_dir_name}"
    cache_dest="$dev_home_dir/${cache_dir_name}"

    reinit_ssh_connection

    echo "Running sync host -> image: ${repo}"

    # run_command_via_ssh "mkdir $dev_home_dir/coreutils"

    copy_file_or_dir_to_device_via_ssh "$repo" "$dev_home_dir"
    [[ -e "$cache_home" ]] && copy_file_or_dir_to_device_via_ssh "$cache_home" "$cache_dest"

    echo "Finished sync host -> image: ${repo}"
}

sync_image() {
    repo="$1"
    cache_home="${HOME}/${cache_dir_name}"
    cache_dest="$dev_probe_dir/${cache_dir_name}"

    reinit_ssh_connection

    echo "Running sync image -> host: ${repo}"

    command="rm -rf $dev_probe_dir/coreutils ${cache_dest}; \
mkdir -p ${cache_dest}; \
cd ${cache_dest}; \
tar czf cargo.tgz -C ~/ .cargo; \
tar czf target.tgz -C ~/coreutils target; \
ls -la ${cache_dest}"
    run_command_via_ssh "$command" || return

    rm -rf "$cache_home"
    copy_file_or_dir_from_device_via_ssh "$cache_dest" "$cache_home" || return

    echo "Finished sync image -> host: ${repo}"
}

build() {
    echo "Running build"

    reinit_ssh_connection

    command="export CARGO_TERM_COLOR=always;
             export CARGO_INCREMENTAL=0; \
             cd ~/coreutils && cargo build --features feat_os_unix_android"
    run_command_via_ssh "$command" || return

    echo "Finished build"
}

tests() {
    echo "Running tests"

    reinit_ssh_connection

    run_script_file_via_ssh "$this_repo/util/android-scripts/run-tests.sh" || return

    echo "Finished tests"
}

hash_rustc() {
    tmp_hash="__rustc_hash__.tmp"
    hash="__rustc_hash__"

    reinit_ssh_connection

    echo "Hashing rustc version: ${HOME}/${hash}"

    command=""
    keep_log=1
    debug=0
    run_command_via_ssh "rustc -Vv" > rustc.log || return
    rm -f "$tmp_hash"
    mv "rustc.log" "$tmp_hash" || return
    # sha256sum is not available. shasum is the macos native program.
    shasum -a 256 "$tmp_hash" | cut -f 1 -d ' ' | tr -d '[:space:]' >"${HOME}/${hash}" || return

    rm -f "$tmp_hash"

    echo "Finished hashing rustc version: ${HOME}/${hash}"
}

#adb logcat &
exit_code=0

if [ $# -eq 1 ]; then
    case "$1" in
        sync_host)
            sync_host "$this_repo"
            exit_code=$?
            ;;
        sync_image)
            sync_image "$this_repo"
            exit_code=$?
            ;;
        build)
            build
            exit_code=$?
            ;;
        tests)
            tests
            exit_code=$?
            ;;
        *) help ;;
    esac
elif [ $# -eq 2 ]; then
    case "$1" in
        snapshot)
            snapshot "$2"
            exit_code=$?
            ;;
        sync_host)
            sync_host "$2"
            exit_code=$?
            ;;
        sync_image)
            sync_image "$2"
            exit_code=$?
            ;;
        *)
            help
            exit 1
            ;;
    esac
elif [ $# -eq 4 ]; then
    case "$1" in
        init)
            shift
            init "$@"
            exit_code=$?
            ;;
        *)
            help
            exit 1
            ;;
    esac
else
    help
    exit_code=1
fi

#pkill adb
exit $exit_code
