#!/usr/bin/env bash
set -eu
bin=$1; shift
cargo build --release --bin $bin
systemd-run --user \
    --pty --same-dir --wait --collect --service-type=exec \
    -p LimitNOFILE=infinity \
    -p CPUWeight=10000 \
    -p LimitMEMLOCK=infinity \
    --unit $bin \
    target/release/$bin

    # -p CPUQuota=100% \
    # -p AllowedCPUs=1 \
