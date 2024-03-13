#!/bin/bash

# spell-checker:ignore nextest watchplus PIPESTATUS

echo "system ressources - RAM:"
free -hm
echo "system ressources - CPU:"
lscpu
echo "system ressources - file systems:"
mount

echo "$HOME"
PATH=$HOME/.cargo/bin:$PATH
export PATH
echo "$PATH"
pwd
command -v rustc && rustc -Vv
ls -la ~/.cargo/bin
cargo --list
cargo nextest --version
