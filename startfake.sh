#!/usr/bin/env bash
set -eu
cargo build --release --bin=fakeupstream
systemctl --user stop fakeupstream || true
systemd-run --user -G --unit=fakeupstream target/release/fakeupstream
