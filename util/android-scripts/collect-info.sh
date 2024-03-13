#!/bin/bash

# spell-checker:ignore nextest watchplus PIPESTATUS

echo "system ressources - file systems:"
df -Th
echo "system ressources - RAM:"
free -hm
echo "system ressources - CPU:"
lscpu

echo "$HOME"
PATH=$HOME/.cargo/bin:$PATH
export PATH
echo "$PATH"
pwd
command -v rustc && rustc -Vv
ls -la ~/.cargo/bin
cargo --list
cargo nextest --version
