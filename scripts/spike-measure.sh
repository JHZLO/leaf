#!/bin/zsh
# Phase 0 measurements over the binaries from spike-build.sh and the files from spike-fixtures.py.
# Each run opens a window in the background and quits by itself; one RESULT line per run.
set -u
ROOT="${0:A:h}/.."
BIN=$ROOT/target/spike/bin
LOGS=$ROOT/target/spike/runs
mkdir -p $LOGS
cd $ROOT/target/spike/fixtures

# run <variant> <label> <exit_ms> <rss_at_ms> <args...>
run() {
  local v=$1 label=$2 exit_ms=$3 rss_at=$4; shift 4
  local log=$LOGS/$v-$label.log
  LEAF_SPIKE_EXIT_MS=$exit_ms $BIN/leaf-spike-$v "$@" 2> $log &
  local pid=$!
  sleep $(( rss_at / 1000.0 ))
  local rss=$(ps -o rss= -p $pid 2>/dev/null | tr -d ' ')
  wait $pid
  local keys=$(grep -oE '(run_ms|init_ms|first_frame_ms|preview_build_ms|preview_frame_ms)=[0-9.]+' $log | tr '\n' ' ')
  local panic=$(grep -m1 -i panic $log)
  echo "RESULT $v $label rss_mb=$(( ${rss:-0} / 1024 )) $keys${panic:+ PANIC: $panic}"
}

for v in a a0 b; do
  for i in 1 2 3 4 5; do run $v start$i 2500 2000 editor hello.md; done
done
for v in a b; do
  for w in 1 0; do
    for size in 1 10 50; do
      exit_ms=$(( size == 50 ? 20000 : 8000 ))
      LEAF_SPIKE_WRAP=$w run $v code${size}mb-wrap$w $exit_ms $(( exit_ms - 1000 )) editor code-${size}mb.rs
    done
    LEAF_SPIKE_WRAP=$w run $v plain10mb-wrap$w 8000 7000 editor plain-10mb.txt
    LEAF_SPIKE_WRAP=$w run $v md10k-wrap$w 4000 3000 editor doc-10k.md
  done
  LEAF_SPIKE_PREVIEW=1 run $v md2k-preview 5000 4000 editor doc-2k.md
  run $v tree10k 3000 2500 tree 10000
done
