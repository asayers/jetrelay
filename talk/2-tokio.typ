#import "@preview/touying:0.6.1": *
#import "util.typ": *

= Tokio + Tunstenite

== Shopping for components

- tokio
    - `broadcast::channel()`
    - Each sent value is seen by all consumers

- tungstenite
    - `connect_async()` - handshake w/ server
    - `accept_async()` - handshake w/ client
    - You get a `Stream<Message>`


== Implementation \#1

// New events arrive from upstream,
// need to be copied to all connected clients

#text(14pt)[
// #set page(columns:2)

```rust
let (mut upstream, _) = connect_async("wss://jetstream...").await?;
let sock = TcpListener::bind("0.0.0.0:80").await?;
let (tx, rx) = channel::<Message>(1024);
```

#grid(columns:2, gutter: 1em,
titled-block(title: [`task`])[
```rust
loop {
    let msg = upstream.next().await?;
    tx.send(msg?)?;
}
```
],
titled-block(title: [`task`])[
```rust
loop {
    let (conn, _) = sock.accept().await?;
    let rx = rx.resubscribe();
    tokio::spawn(async move {
        let mut ws = accept_async(conn).await?;
        while let Ok(msg) = rx.recv().await {
            ws.send(msg).await?;
        }
    });
}
```
])

]

== Broadcast channel

#image("broadcast_1.svg", height: 50%)
---
#image("broadcast_2.svg", height: 50%)
---
#image("broadcast_3.svg", height: 50%)
---
#image("broadcast_4.svg", height: 50%)
---
#image("broadcast_5.svg", height: 50%)
---
#image("broadcast_6.svg", height: 50%)
---
#image("broadcast_7.svg", height: 50%)
---
#image("broadcast_8.svg", height: 50%)
---
#image("broadcast_9.svg", height: 50%)
---
#image("broadcast_10.svg", height: 50%)
---
#image("broadcast_11.svg", height: 50%)
---
#image("broadcast_12.svg", height: 50%)
---
#image("broadcast_13.svg", height: 50%)

#speaker-note[
Customise to support cursor=...
]

== How does it do?

---

#v(1em)
#align(center)[
#text(red.darken(30%))[
```
Error: Too many open files (os error 24)
```
]]
#v(1em)

#pause

- `ulimit -n`
- `LimitNOFILE`
- `rlimit::setrlimit(Resource::NOFILE)`

#pause

```console
systemd-run --user -p LimitNOFILE=100000 -p CPUQuota=100%
```

#speaker-note[
Single core
loopback interface
]

---

// Can manage \~9.8 Gbps

// Starts lagging around 6.5k clients

// Can't _quite_ saturate a 10G NIC

#table(columns:3, inset: 0.5em,
table.header([*Impl*], [*Throughput*], [*Clients*]),
[tokio+tungstenite], [9.8 Gbps], [6.5k],
[???], [??? Gbps], [???],
[???], [??? Gbps], [???],
[???], [??? Gbps], [???],
)

#small[(single core, loopback interface)]

// Performance is not bad!

// 13.2 Gbps (on loopback with 20% of a core)

// Can we do better...?




// Now when one client is slow it won't affect the others
// we'll buffer events for them in the channel (indefinitely...?)

// What's bad about this?
//
// * copy copy copy
// * threading overhead (you could translate this to async if you wanted)
//
// And we haven't even implemented backfill!
// Remember, a connecting client can request to be backfilled with
// old events
// We might be sending out data which is minutes or hours old
// => a copy of all the event data will need to be saved to disk
// 
// And by the way I didn't design this to be a straw-man punching-bag
// I actually think this is not unreasonable
// And in fact the official upstream server basically works like this
// (albeit with goroutines instead of OS threads)
