export PATH=\$HOME/.cargo/bin:\$PATH
export RUST_BACKTRACE=1
export CARGO_TERM_COLOR=always
export CARGO_INCREMENTAL=0

function watchplus {
    # call: watchplus <interval> <command>
    while true; do
        "${@:2}";
        sleep $1;
    done
}

watchplus 2 "df -h; free -m" &

cd ~/coreutils && \
timeout --preserve-status --verbose -k 1m 60m \
cargo nextest run --profile ci --hide-progress-bar --features feat_os_unix_android
