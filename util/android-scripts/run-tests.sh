#!/bin/bash

echo "PATH: $PATH"

export PATH=$HOME/.cargo/bin:$PATH
export RUST_BACKTRACE=1
export CARGO_TERM_COLOR=always
export CARGO_INCREMENTAL=0

echo "PATH: $PATH"

watchplus() {
    # call: watchplus <interval> <command>
    while true; do
        "${@:2}"
        sleep "$1"
    done
}

kill_all_background_jobs() {
    jobs -p | xargs -I{} kill -- {}
}

watchplus 2 df -h &
watchplus 2 free -hm &

cd ~/coreutils && \
    timeout --preserve-status --verbose -k 1m 60m \
        cargo nextest run --profile ci --hide-progress-bar --features feat_os_unix_android

result=$?

kill_all_background_jobs

return $result
