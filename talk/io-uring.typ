#import "@preview/touying:0.6.1": *
#import "util.typ": *

= io_uring

== io_uring

```rust
let mut uring = IoUring::builder()
    .setup_single_issuer()
    .setup_coop_taskrun()
    .setup_defer_taskrun()
    .build(8192)?;
```

---

#grid(columns:(1fr, 1fr))[
```rust
libc::write(
    conn.as_raw_fd(),
    buf.as_ptr(),
    buf.len(),
);
```

#v(1fr)
#text(black.lighten(50%))[
```
  conn: TcpStream
  buf:  Vec<u8>
```]
#v(1fr)

][
```rust
let sqe = opcode::Write::new(
    conn.as_raw_fd(),
    buf.as_ptr(),
    buf.len(),
).build();
```
#only(1)[
```rust
uring.submission().push(sqe);
uring.submit_and_wait(1);
```]

#only("2-")[
```rust
uring.submission().push(sqe);
uring.submission().push(sqe2);
uring.submit_and_wait(1);
```]

#only(3)[
```rust
for cqe in uring.completion() {
    // ...
}
```
]]

== Performance

// == Thundering herd

// `CVAR.notify_all()` \
// => thousands of threads fight over the mutex

// == io_uring

#table(columns:3, inset: 0.5em,
table.header([*Implementation*], [*Throughput*], [*Clients*]),
[Non-blocking I/O], [10 Gbps], [6.5k],
[\+ batching], [56 Gbps], [35k],
[\+ zero-copy], [80 Gbps], [50k],
[Async I/O], [90+ Gbps], [60k+],
)

#small[(single core, loopback interface)]

#speaker-note[
Now... I will say that the uring-based version was the hardest implementation to write, but a long way.
]

== Ring-specific fds


