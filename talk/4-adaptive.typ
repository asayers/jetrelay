#import "@preview/touying:0.6.1": *
#import "util.typ": *


= Adaptive batching

== Events, what events?
// Removing boundaries

TODO: Show the `tokio::broadcast` channel with its events

Transform to a vec of bytes

== Implementation \#2

#text(17pt)[

#grid(columns:2, gutter: 1em,
[
```rust
let bytes: Vec<u8> = Vec::with_capacity(1 << 30);
let (mut upstream, _) = connect_async("...").await?;
let sock = TcpListener::bind("0.0.0.0:80").await?;
static NOTIFY: Notify = Notify::const_new();
```

#titled-block(title: [`task`])[
```rust
loop {
    let msg = upstream.next().await?;
    bytes.extend(ws_header(&msg))?;
    bytes.extend(&msg)?;
    NOTIFY.notify_waiters();
}
```
]],
titled-block(title: [`task`])[
```rust
loop {
    let (conn, _) = sock.accept().await?;
    tokio::spawn(async move {
        let ws = accept_async(conn).await?;
        let mut conn = ws.into_inner();
        let mut offset = 0;
        loop {
            NOTIFY.notified().await;
            let xs = &bytes[offset..];
            let n = conn.write(xs).await?;
            offset += n;
        }
    });
}
```
])

            // match conn.write(&bytes[offset..]).await {
            //     Ok(n) =>  offset += n,
            //     Err(e) if e.kind() == ErrorKind::WouldBlock => (), // loop
            //     Err(e) => break,
            // }
]

// // We write the data to the file
// // _exactly_ as it'll appear on the wire
// // That means: including the websocket framing bytes


== Adaptive batching

#v(1fr)
Client is *keeping up* ⇒  send *\~500 B* chunks ⇒  *low latency*

#align(center)[
↑ \
⋮ \
↓
]

Client is *way behind* ⇒  send *1 MiB* chunks ⇒ *high throughput*
#v(1fr)

== How does it do?

---

#v(1em)
#align(center)[
#text(red.darken(30%))[
```
Error: Cannot assign requested address (os error 99)
```
]]
#v(1em)

#pause

```
sysctl -w net.ipv4.ip_local_port_range="1024 65535"
```

---

#table(columns:3, inset: 0.5em,
table.header([*Implementation*], [*Throughput*], [*Clients*]),
[Non-blocking I/O], [10 Gbps], [6.5k],
[\+ batching], [56 Gbps], [35k],
[???], [??? Gbps], [???],
[???], [??? Gbps], [???],
)

#small[(single core, loopback interface)]

#speaker-note[
One thing to note:
We're trying to measure the max number of clients our implementation can support, right?
But now, as the server gets overloaded, we have a gradual degradation of service quality
As in, the clients will start recieving an increasingly choppy feed.
So the questions of "max clients" becomes a bit ambiguous.
Like, if we can keep 40k clients fed, but they're all 10s behind, does that count?
I just made a judgement about:
- "it's keeping all clients fed smoothly"
- vs.
- "clients are being fed in large, sporadic bursts".
]

