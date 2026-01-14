#!/usr/bin/env -S bash -eu

cargo build --release --bin=naiverelay
cmd="$PWD/target/release/naiverelay"

properties=(
    # The port naiverelay will to listen on
    "Environment=JETRELAY_PORT=7376"
    # Set the URL of the upstream server
    "Environment=UPSTREAM_URL=ws://localhost:7375/subscribe"
    # # Give it somewhere to keep its files
    # "RuntimeDirectory=naiverelay"
    # Raise the fd limit
    "LimitNOFILE=1000000"
    "CPUQuota=20%"
    "CPUWeight=10000"
    "CollectMode=inactive-or-failed"
)

systemd-run \
    --user \
    --unit=naiverelay \
    --pty \
    --service-type=exec \
    -E IMPL -E RUST_LOG \
    "${properties[@]/#/-p}" \
    $cmd
