#!/usr/bin/env -S bash -eu

cargo build --release --bin=jetrelay
cmd="$PWD/target/release/jetrelay"

properties=(
    # The port jetrelay will to listen on
    "Environment=JETRELAY_PORT=7375"
    # Set the URL of the upstream server
    # "Environment=UPSTREAM_URL=wss://jetstream2.us-west.bsky.network/subscribe"
    # # Give it somewhere to keep its files
    # "RuntimeDirectory=jetrelay"
    # Raise the fd limit
    "LimitNOFILE=1000000"
    "CPUQuota=20%"
    "CPUWeight=10000"
    "CollectMode=inactive-or-failed"
)

systemd-run \
    --user \
    --unit=jetrelay \
    --pty \
    --service-type=exec \
    -E IMPL -E RUST_LOG \
    "${properties[@]/#/-p}" \
    $cmd
