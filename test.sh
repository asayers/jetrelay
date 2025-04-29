#!/usr/bin/env -S bash -eu

musl=x86_64-unknown-linux-musl
cargo build --release --target=$musl --bin=jetrelay
cmd="$PWD/target/$musl/release/jetrelay"

properties=(
    # The port jetrelay will to listen on
    "Environment=JETRELAY_PORT=7375"
    # Set the URL of the upstream server
    "Environment=UPSTREAM_URL=wss://jetstream2.us-west.bsky.network/subscribe"
    # Give it somewhere to keep its files
    "RuntimeDirectory=jetrelay"
    # Raise the fd limit
    "LimitNOFILE=65535"

    # We want to protect $HOME to prevent a buggy relay from exposing
    # private data on the network. But $cmd (probably) lives inside $HOME!
    # So we bind-mount $cmd back in to the namespace afterwards.  But
    # ProtectHome=yes makes $HOME unavailable even for bind-mounting.  So we use
    # ProtectHome=tmpfs instead.
    "ProtectHome=tmpfs"
    "BindPaths=$cmd"

    "CollectMode=inactive-or-failed"

    # Lock it down.  This stuff comes from portabled's "default" profile.
    "RemoveIPC=yes"
    "PrivateDevices=yes"
    "PrivateUsers=yes"
    "ProtectSystem=strict"
    "ProtectKernelTunables=yes"
    "ProtectKernelModules=yes"
    "ProtectControlGroups=yes"
    "LockPersonality=yes"
    "MemoryDenyWriteExecute=yes"
    "RestrictRealtime=yes"
    "RestrictNamespaces=yes"
    "SystemCallFilter=@system-service"
    "SystemCallErrorNumber=EPERM"
    "SystemCallArchitectures=native"
    "RestrictAddressFamilies=AF_UNIX AF_NETLINK AF_INET AF_INET6" \
    "CapabilityBoundingSet=CAP_CHOWN CAP_DAC_OVERRIDE CAP_DAC_READ_SEARCH CAP_FOWNER \
            CAP_FSETID CAP_IPC_LOCK CAP_IPC_OWNER CAP_KILL CAP_MKNOD CAP_NET_ADMIN \
            CAP_NET_BIND_SERVICE CAP_NET_BROADCAST CAP_SETGID CAP_SETPCAP \
            CAP_SETUID CAP_SYS_ADMIN CAP_SYS_CHROOT CAP_SYS_NICE CAP_SYS_RESOURCE" \
    "MountAPIVFS=yes"
    "BindLogSockets=yes"
    "BindReadOnlyPaths=/etc/machine-id"
    "BindReadOnlyPaths=-/etc/resolv.conf"
    "BindReadOnlyPaths=/run/dbus/system_bus_socket"
    # I don't think systemd-run --user supports DynamicUser=yes
)

systemd-run \
    --user \
    --unit=jetrelay \
    --pty \
    --service-type=exec \
    "${properties[@]/#/-p}" \
    $cmd
