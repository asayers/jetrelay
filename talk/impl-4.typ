#import "@preview/touying:0.6.1": *
#import "util.typ": *

#show raw: set text(15pt)
#set page(columns:2)

= Impl 4

// #codeslide[
//   ```rust
//   static BYTES: Mutex<Vec<u8>>;
//   static CVAR: Condvar;
//   static INDEX: Mutex<BTreeMap<Timestamp, usize>>;
//   static CLIENTS: Mutex<Vec<(TcpConnection, usize)>>;
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
//       let timestamp = websocket_handshake(&conn);
//       let mut cursor = INDEX.lock()?.get(timestamp);
//       CLIENTS.lock()?.push((conn, cursor));
//   }
//   ```
//   ]

//   #titled-block(title: [`all-clients thread`])[
//   ```rust
//   let uring = IoUring::new(1024)?;
//   loop {
//       let sq = uring.submission();
//       for (conn, cursor) in &mut CLIENTS.lock()? {
//           sq.push(opcode::Write)
//           let n = conn.write_all(&bytes[cursor..])?;
//           cursor += n;
//       }
//   }
//   ```
//   ]
// ]
