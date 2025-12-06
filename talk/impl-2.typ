#import "@preview/touying:0.6.1": *
#import "util.typ": *

#show raw: set text(15pt)
#set page(columns:2)

= Impl 2

#slide[
  ```rust
  static BYTES: Mutex<Vec<u8>>;
  static CVAR: Condvar;
     
  ```

  // We write the data to the file
  // _exactly_ as it'll appear on the wire
  // That means: including the websocket framing bytes
  
  #titled-block(title: [`events thread`])[
  ```rust
  loop {
      let event = recv_from_upstream();
      let frame = add_ws_framing(event);
      let bytes = BYTES.lock()?;

      bytes.extend(&frame)?;
      CVAR.notify_all();
  }
  ```
  ]
  
  #pause
  #colbreak()
  #titled-block(title: [`new client thread`])[
  ```rust
  loop {
      let conn = listener.accept()?;
      thread::spawn(|| client_thread(conn));
  }
  ```
  ]

  #titled-block(title: [`per-client thread`])[
  ```rust
  websocket_handshake(&conn);
  let bytes = BYTES.lock()?;
  let mut cursor = bytes.len();
  loop {
      CVAR.wait(bytes);
      let n = conn.write_all(&bytes[cursor..])?;
      cursor += n;
  }
  ```
  ]
]

#slide[
  ```rust
  static BYTES: Mutex<Vec<u8>>;
  static CVAR: Condvar;
  static INDEX: Mutex<BTreeMap<Timestamp, usize>>;
  ```

  #titled-block(title: [`events thread`])[
  ```rust
  loop {
      let event = recv_from_upstream();
      let frame = add_ws_framing(event);
      let bytes = BYTES.lock()?;
      INDEX.lock().insert(event.ts, bytes.len());
      bytes.extend(&frame)?;
      CVAR.notify_all();
  }
  ```
  ]
  
  #colbreak()
  #titled-block(title: [`new client thread`])[
  ```rust
  loop {
      let conn = listener.accept()?;
      thread::spawn(|| client_thread(conn));
  }
  ```
  ]

  #titled-block(title: [`per-client thread`])[
  ```rust
  let timestamp = websocket_handshake(&conn);
  let bytes = BYTES.lock()?;
  let mut cursor = INDEX.lock()?.get(timestamp);
  loop {
      CVAR.wait(bytes);
      let n = conn.write_all(&bytes[cursor..])?;
      cursor += n;
  }
  ```
  ]
]
