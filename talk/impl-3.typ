#import "@preview/touying:0.6.1": *
#import "util.typ": *

#show raw: set text(15pt)
#set page(columns:2)

= Impl 3

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

#slide[
  ```rust
  static FILE: Mutex<File>;
  static FILE_LEN: AtomicUsize;
      
  ```

  #titled-block(title: [`new client thread`])[
  ```rust
  loop {
      let conn = listener.accept()?;
      thread::spawn(|| client_thread(conn));
  }
  ```
  ]

  #pause

  // We write the data to the file
  // _exactly_ as it'll appear on the wire
  // That means: including the websocket framing bytes
  
  #titled-block(title: [`events thread`])[
  ```rust
  loop {
      let event = recv_from_upstream();

      let frame = add_ws_framing(event);
      let n = FILE.write_all(&frame)?;
      FILE_LEN.fetch_add(n);
  }
  ```
  ]

  #pause

  #colbreak()
  #titled-block(title: [`per-client thread`])[
  ```rust
  websocket_handshake(&conn);
  let mut buf = vec![0; 4096];
  loop {
      if cursor < FILE_LEN.load() {
          FILE.read_at(&mut buf, cursor)?;
          let n = conn.write_all(&buf)?;
          cursor += n;
      } else {
          thread::sleep(10);
      }
  }
  ```
  ]
]

// We're no longer copying the event data into thousands of buffers
// But we are reading it in from the file a thousand times
// Luckily, there's a syscall for just this situation
// It's called sendfile()

#slide[
  ```rust
  static FILE: Mutex<File>;
  static FILE_LEN: AtomicUsize;
    
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

      let frame = add_ws_framing(event);
      let n = FILE.write_all(&frame)?;
      FILE_LEN.fetch_add(n);
  }
  ```
  ]

  #colbreak()
  #titled-block(title: [`per-client thread`])[
  ```rust
  websocket_handshake(&conn);

  loop {
      if cursor < FILE_LEN.load() {

          let n = sendfile(sock, file, cursor, usize::MAX)?;
          cursor += n;
      } else {
          thread::sleep(10);
      }
  }
  ```
  ]
]


// By the way, for functions like `sendfile()`, I highly recommend the "rustix" crate

// Now, let's think through what happens when an event arrives
// 1. We receive the event data
// 2. We parse the JSON, at least enough to extract the timestamp
// 3. We push to the index.  This involves taking a lock, but the lock is almost always free.  It'll only be contested if new clients are connecting
// 4. We write the event data to file.  The `write()` syscall copies the data out of our address space and into the page cache.  (Actual disk I/O will happen lazily, much later)
// 5. We bump the atomic.  This will allow the per-client workers to pick up the new data
// 6. As the worker threads wake up, they'll see new file length and realise they're behind.
// 7. They will make `sendfile()` syscalls until all clients are up-to-date.

#slide[
  ```rust
  static FILE: Mutex<File>;
  static FILE_LEN: AtomicUsize;
  static INDEX: Mutex<HashMap<Timestamp, Offset>>;
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
      INDEX.insert(FILE_LEN.load(), event.ts);
      let frame = add_ws_framing(event);
      let n = FILE.write_all(&frame)?;
      FILE_LEN.fetch_add(n);
  }
  ```
  ]

  #colbreak()
  #titled-block(title: [`per-client thread`])[
  ```rust
  let timestamp = websocket_handshake(&conn);
  let cursor = INDEX.lookup(timestamp);
  loop {
      if cursor < FILE_LEN.load() {

          let n = sendfile(sock, file, cursor, usize::MAX)?;
          cursor += n;
      } else {
          thread::sleep(10);
      }
  }
  ```
  ]
]

// Let's think through what happens when a client connects
// 1. We do the handshake (on a separate thread, so it doesn't impact the rest of the system)
// 2. The client has requested a certain timestamp, so we look it up in the index
// 3. This gives us the initial value for the client's cursor
// 4. The worker thread now calls `sendfile()` to get the client caught up
// 5. Once it's up-to-date, the worker goes to sleep
//
// Note: There's no difference between a client which is being backfilled, a client which is overloaded and slowly catching up, or a client which is at the cutting-edge.
