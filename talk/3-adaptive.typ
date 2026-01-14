#import "@preview/touying:0.6.1": *
#import "util.typ": *


= Adaptive batching

== Events, what events?
// Removing boundaries

TODO: Show the `tokio::broadcast` channel with its events

Transform to a vec of bytes

== Implementation \#2

#text(14pt)[#columns(2)[

```rust
let (mut upstream, _) = ws_connect("wss://jetstream...")?;
let sock = TcpListener::bind("0.0.0.0:80")?;
let bytes: Vec<u8> = vec![];
```

// ```rust
// static BYTES: Mutex<Vec<u8>>;
// static CVAR: Condvar;
// ```

// // We write the data to the file
// // _exactly_ as it'll appear on the wire
// // That means: including the websocket framing bytes

// #titled-block(title: [`events thread`])[
// ```rust
// loop {
//   let event = recv_from_upstream();
//   let frame = add_ws_framing(event);
//   let bytes = BYTES.lock()?;

//   bytes.extend(&frame)?;
//   CVAR.notify_all();
// }
// ```
// ]

// #colbreak()
// #titled-block(title: [`new client thread`])[
// ```rust
// loop {
//   let conn = listener.accept()?;
//   thread::spawn(|| client_thread(conn));
// }
// ```
// ]

// #titled-block(title: [`per-client thread`])[
// ```rust
// websocket_handshake(&conn);
// let bytes = BYTES.lock()?;
// let mut cursor = bytes.len();
// loop {
//   CVAR.wait(bytes);
//   let n = conn.write_all(&bytes[cursor..])?;
//   cursor += n;
// }
// ```
// ]

]]


// #slide[
//   ```rust
//   static BYTES: Mutex<Vec<u8>>;
//   static CVAR: Condvar;
//   static INDEX: Mutex<BTreeMap<Timestamp, usize>>;
//   ```

//   #titled-block(title: [`events thread`])[
//   ```rust
//   loop {
//       let event = recv_from_upstream();
//       let frame = add_ws_framing(event);
//       let bytes = BYTES.lock()?;
//       INDEX.lock().insert(event.ts, bytes.len());
//       bytes.extend(&frame)?;
//       CVAR.notify_all();
//   }
//   ```
//   ]
  
//   #colbreak()
//   #titled-block(title: [`new client thread`])[
//   ```rust
//   loop {
//       let conn = listener.accept()?;
//       thread::spawn(|| client_thread(conn));
//   }
//   ```
//   ]

//   #titled-block(title: [`per-client thread`])[
//   ```rust
//   let timestamp = websocket_handshake(&conn);
//   let bytes = BYTES.lock()?;
//   let mut cursor = INDEX.lock()?.get(timestamp);
//   loop {
//       CVAR.wait(bytes);
//       let n = conn.write_all(&bytes[cursor..])?;
//       cursor += n;
//   }
//   ```
//   ]
// ]

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
[tokio + tungstenite], [9.8 Gbps], [6.5k],
[vec + non-blocking write], [56 Gbps], [35k],
[???], [??? Gbps], [???],
[???], [??? Gbps], [???],
)

#small[(single core, loopback interface)]

