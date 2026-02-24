#import "@preview/touying:0.6.1": *
#import "util.typ": *

= Baseline impl

== Shopping for components

- tungstenite
    - `connect_async()` - handshake w/ server
    - `accept_async()` - handshake w/ client
    - Both return `impl Stream<Message> + Sink<Message>`

#pause

- `broadcast::channel()` (from tokio)
    - Sent values seen by all consumers

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

== Implementation \#1

// New events arrive from upstream,
// need to be copied to all connected clients

#text(18pt)[
// #set page(columns:2)

```rust
let sock = TcpListener::bind("0.0.0.0:80").await?;
let (mut upstream, _) = connect_async("wss://jetstream...").await?;
let (tx, rx) = broadcast::channel::<Message>(1024);
```

#grid(columns:(2fr, 3fr), gutter: 1em,
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
    let from_firehose = rx.resubscribe();
    tokio::spawn(async move {
        let mut to_client = accept_async(conn).await?;
        while let Ok(msg) = from_firehose.recv().await {
            to_client.send(msg).await?;
        }
    });
}
```
])

]

== Performance

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
systemd-run --user -p LimitNOFILE=100000
```

---

#let unknown = text(gray)[???]
#align(center,
table(columns:3, inset: 0.4em, stroke:none,
table.header([*Impl*], [*Clients*], [*Throughput*]),
table.hline(),
[\#1], [2.7k], [4 Gbps], // [6.5k], [10 Gbps],
[\#2], unknown, unknown,
[\#3], unknown, unknown,
[\#4], unknown, unknown,
))

#speaker-note[
Can manage \~9.8 Gbps
Starts lagging around 6.5k clients
Not bad for such straightforward code!
Off-the shelf components
If your server has a 10G NIC then you're almost at the physical limit
...but amazon will happily rent you a machine with a 200G NIC
They'll do you a machine with a network interface measured in _terrabits_! (if you've got the cash)
On a 100G machine you should be able to serve 65k simultaneous clients
...if the software can keep up
so can we do better?  (The ??? boxes are a give-away)
]

// == Theoretical limits

// #speaker-note[
// The hard upper bound is given by the network interface...

// Your machine probably has a 1-gigabit network card.  That can do 600 clients
// If you buy a server it will probably come with a 10-gig card.
// Amazon will rent you a machine with a 100- or even 200-gig NIC!

// ]

// 1Gbps NIC => \~600 simulteneous clients

// 10Gbps NIC => \~6000 simulteneous clients

// 100Gbps NIC => \~60k simulteneous clients!

// #speaker-note[
// ...but what about the software?
// can we write a program which can support 10s of thousands of clients?
// ]







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
