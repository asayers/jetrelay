#import "@preview/touying:0.6.1": *
#import "util.typ": *

#show raw: set text(15pt)

= Impl 1

#slide[

// New events arrive from upstream,
// need to be copied to all connected clients

// `send_to_client()` adds the websocket framing and writes to the TCP
// connection

```rust
static CLIENTS: Mutex<Vec<TcpStream>>;
```

#titled-block(title: [`new client thread`])[
```rust
loop {
  let conn = listener.accept()?;
  websocket_handshake(&conn);
  CLIENTS.lock()?.push(conn);
}
```
]

#pause
#titled-block(title: [`events thread`])[
```rust
loop {
  let event = recv_from_upstream();
  for conn in CLIENTS.lock()? {
      let frame = add_ws_framing(event);
      conn.write_all(&frame)?;
  }
}
```
]
][...]

#slide[

// But: clients can (temporarily) fall behind,
// and sending will block

```rust
static CLIENTS: Mutex<Vec<Receiver<Json>>>;
```

#titled-block(title: [`new client thread`])[
```rust
loop {
    let conn = listener.accept()?;
    thread::spawn(|| client_thread(conn));
}
```
]

#titled-block(title: [`events thread`])[
```rust
loop {
    let event = recv_from_upstream();
    for tx in CLIENTS.lock()? {
        tx.send(event);
    }
}
```
]

][

#titled-block(title: [`per-client thread`])[
```rust
websocket_handshake(&conn);
let (tx, rx) = mpsc::channel();
CLIENTS.lock()?.push(tx);
while let Ok(event) = rx.recv() {
    let frame = add_ws_framing(event);
    conn.write_all(&frame)?;
}
```
]
]

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
