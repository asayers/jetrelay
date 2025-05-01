<h1 align="center">Jetrelay</h1>
<p align="center">A basic jetstream relay</p>

See [here](https://www.asayers.com/jetrelay.html) for an explanation.

Jetrelay reads the following env vars:

* `JETRELAY_PORT` (**required**) - the port to listen to
* `UPSTREAM_URL` (**required**) - the upstream relay to mirror
* `RUNTIME_DIRECTORY` (**required**) - the directly to keep runtime data in
* `RUST_LOG` - logging level ("warn", "debug", etc.)

Also, each client consumes 3 fds, so you'll want to increase the fd limit if you
expect a lot of clients.

### Quick start

Using `systemd-run`:

```console
$ cargo build --release
$ systemd-run --user --pty \
    -EJETRELAY_PORT=7375 \
    -EUPSTREAM_URL="wss://<some jetstream server>/subscribe" \
    -pRuntimeDirectory=jetrelay \
    -pLimitNOFILE=65535 \
    ./target/release/jetrelay
```

Or the old-school way (pollutes your shell's environment):

```console
$ cargo build --release
$ export JETRELAY_PORT=7375
$ export RUNTIME_DIRECTORY=$(mktemp -d)
$ export UPSTREAM_URL="wss://<some jetstream server>/subscribe"
$ ulimit -n 65535
$ ./target/release/jetrelay
```
