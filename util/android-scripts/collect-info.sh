#!/bin/bash

# spell-checker:ignore nextest watchplus PIPESTATUS

echo -n "uname -a: " && uname -a

echo "$HOME"
PATH=$HOME/.cargo/bin:$PATH
export PATH
echo "$PATH"
pwd
command -v rustc && rustc -Vv
ls -la ~/.cargo/bin
cargo --list
cargo nextest --version
