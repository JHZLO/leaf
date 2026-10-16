#!/bin/zsh
# Phase 0: clean release builds of the three spike variants, one after another, timed.
#   a   gpui-kit with the 17 grammars
#   a0  the same without grammars (built after a, so its time is incremental)
#   b   a on GPUI Fast
# Binaries land in target/spike/bin, build logs next to them.
set -u
cd "${0:A:h}/.."
BIN=target/spike/bin
mkdir -p $BIN
rm -rf target/a target/b

step() {
  local name=$1; shift
  local t0=$(date +%s)
  if cargo build --release "$@" > $BIN/$name.log 2>&1; then
    echo "build $name ok seconds=$(( $(date +%s) - t0 ))"
  else
    echo "build $name FAILED seconds=$(( $(date +%s) - t0 ))"; tail -20 $BIN/$name.log; return 1
  fi
}

step a --target-dir target/a && cp target/a/release/leaf-spike $BIN/leaf-spike-a
step a0 --target-dir target/a --no-default-features && cp target/a/release/leaf-spike $BIN/leaf-spike-a0
# Leave target/a holding the full build, which is what you run by hand.
cargo build --release --target-dir target/a > /dev/null 2>&1
step b --target-dir target/b --features fast && cp target/b/release/leaf-spike $BIN/leaf-spike-b
ls -l $BIN | grep leaf-spike | awk '{print $9, $5}'
