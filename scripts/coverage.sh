#!/usr/bin/env bash

set -exEuo pipefail

dir=$(mktemp -d)

cargo tarpaulin \
    --engine llvm \
    --locked \
    --all-features --ignore-tests --lib \
    -o html --output-dir "$dir"

xdg-open "$dir/tarpaulin-report.html" &
