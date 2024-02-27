#!/bin/bash

# spell-checker:ignore nextest watchplus PIPESTATUS

echo "information about AVD:"

log_and_run() {
    echo "$*:"
    "$@"
}

log_and_run lscpu
log_and_run cat /proc/cpuinfo
log_and_run echo "$HOME"
PATH=$HOME/.cargo/bin:$PATH
export PATH
log_and_run echo "$PATH"
log_and_run pwd
log_and_run command -v rustc && rustc -Vv
log_and_run ls -la ~/.cargo/bin
log_and_run cargo --list
log_and_run cargo nextest --version
