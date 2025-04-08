#!/usr/bin/env -S bash -eu

musl=x86_64-unknown-linux-musl
cargo build --release --target=$musl --bin=jetrelay

ROOT=$(mktemp -d)
(
    cd $ROOT
    mkdir -p etc proc sys dev run tmp var/tmp usr/bin usr/lib/systemd/system
    touch etc/resolv.conf etc/machine-id
    echo -e "ID=barebones\nVERSION_ID=1" >etc/os-release
)
cp systemd/jetrelay.* $ROOT/usr/lib/systemd/system/
cp target/$musl/release/jetrelay $ROOT/usr/bin/

output=target/jetrelay.raw
rm -f $output
mksquashfs $ROOT $output
