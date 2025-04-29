#!/usr/bin/env -S bash -eu
musl=x86_64-unknown-linux-musl
cargo build --release --target=$musl --bin=jetrelay
systemd-run \
    --user \
    --unit=jetrelay \
    --wait \
    --collect \
    --service-type=exec \
    -pLimitNOFILE=65535 \
    -EJETRELAY_PORT=7375 \
    -pRuntimeDirectory=jetrelay \
    ./target/$musl/release/jetrelay

    # -ERUST_LOG=trace \
