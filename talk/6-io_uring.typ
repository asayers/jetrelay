#import "@preview/touying:0.6.1": *
#import "util.typ": *

= io_uring

== `write() write() write()`

#speaker-note[
We call write(), it returns a result, etc.
Each time we do a syscall, there's some overhead
(mostly spent on Spectre/Meltdown mitigations.)

But look: none of these depend on each other.
We're going to do the same three writes no matter what.
So wouldn't it be nicer if we could do this instead?
]

#grid(columns: 2,
[
#v(1fr)
#align(center, image("io_uring_1.svg", width: 60%))
#v(1fr)
],
[
#pause
#v(1fr)
#align(center, image("io_uring_2.svg", width: 60%))
#v(1fr)
])

#speaker-note[
Here the idea is to batch up the three writes and submit them together.
Just a single syscall which tells the kernel to do multiple things.
We have certain special-case things like vectorized I/O for writing multiple buffers to the same socket
But there's nothing that specifically covers the case of writing to _mutliple_ sockets.
Wouldn't it be nice if there was a generic way of describing an arbitrary set of syscalls and sending that to the kernel...?
]

== io_uring

// Normal syscalls:
//     ┌----[write($x)]-------⬎
// userspace             kernelspace
//   	⬑ -------[ok]-----------┘
//     ┌----[write($y)]-------⬎
// userspace             kernelspace
//  	⬑ -------[err]----------┘
//     ┌----[write($z)]-------⬎
// userspace             kernelspace
//  	⬑ -------[ok]-----------┘
// With io_uring:
// 	        [write($x)]
//     ┌----[write($y)]-------⬎
//     |    [write($z)]        |
// userspace             kernelspace
//  	|        [ok ]          |
//  	⬑ -------[err]----------┘
// 	           [ok ]

#v(1cm)
#align(center, [
// squeue:
#image("io_uring_squeue.svg", width: 60%)
])
#v(1cm)
#align(center, [
// cqueue:
#image("io_uring_cqueue.svg", width: 60%)
])

#speaker-note[
You get two channels:
- one going from userspace to the kernel (squeue)
- one coming from the kernel back to userspace (cqueue)
]


== SQEs

#grid(columns:(1fr, 1fr))[
*Syscall*
```rust
libc::write(
    conn.as_raw_fd(),
    buf.as_ptr(),
    buf.len(),
);
```
][
*SQE*
```rust
let sqe = opcode::Write::new(
    conn.as_raw_fd(),
    buf.as_ptr(),
    buf.len(),
).build();
```
]

Most syscalls have a corresponding SQE

#pause

...but not `sendfile()` 😔

== CQEs

```rust
uring.submission().push(sqe);

uring.submit_and_wait(1);

let cqe = uring.completion().next().unwrap();
println!("Result was {}", cqe.result()?);
```

#speaker-note[
You push descriptions of I/O ops to the squeue.  When you're ready to submit them, you call
`submit_and_wait()`, which is a syscall (`io_uring_enter()`).
At this point the kernel will loop over the I/Os in the squeue, running them
just as if you had called `write()` in a loop yourself.
Instead of returning a result, it pushes the results onto the cqueue.
Once `submit_and_wait()` returns, you look at what's in the cqueue to find out how your I/Os went.

This describes what happens in practice when none of your I/Os block (such as in our case, where we're doing non-blocking sends).
However, what if we submit _blocking_ I/Os?
The semantics of the original syscalls-in-a-loop code would be to delay subsequent I/Os until the preceding one completes.
That's not what io_uring does though.

Let's suppose we're doing blocking sends.
If the socket has space free, it performs the send immediately and moves on to the next I/O.
If the socket is full, however, it tucks the I/O away for later and registers a callback in the socket's wake queue, before moving on to the next I/O in the squeue.
Later, when the socket gets unclogged, this callback will mark the I/O as "retryable".
This retry will happen immediately if your process is still blocking on `submit_and_wait()`---the callback also re-schedules your thread.
Or if not, it'll happen the next time you call `submit_and_wait()`.

In effect, it's exactly what you'd do with epoll if you were doing it from userspace.
You can almost think of io_uring as a Tokio reactor implemented in kernelspace.

(Caveat: I'm describing the behaviour of recent kernels)
]

== User data

```rust
squeue.push(sqe1.user_data(1));
squeue.push(sqe2.user_data(2));
uring.submit_and_wait(2);
for cqe in uring.completion() {
    println!(
        "SQE {} returned {}",
        cqe.user_data(),
        cqe.result()?,
    );
}
```

== Implementation \#4

#text(17pt)[
```rust
static DATA: Mutex<Vec<u8>> = Mutex::new(Vec::with_capacity(16 << 30));
static CLIENTS: Mutex<Slab<Client>> = Mutex::new(Slab::new());
```

#grid(columns:2, gutter: 1em,
[
```rust
let sock = TcpListener::bind("0.0.0.0:80")?;
let mut upstream = connect_websocket("...")?;
let mut uring = IoUring::builder().build(8<<10)?;
```
#titled-block(title: [`thread`])[
```rust
loop {
    let msg = upstream.next()?;
    let msg = add_ws_framing(msg);
    DATA.extend(&msg)?;
    assert!(DATA.len() <= 16 << 30);
}
```
]],
titled-block(title: [`thread`])[
```rust
loop {
    let (conn, _) = sock.accept()?;
    thread::spawn(move {
        accept_websocket(&conn)?;
        CLIENTS.insert(Client {
            conn,
            offset: DATA.len(),
            in_flight: false,
        });
    });
}
```
],
)
]

#slide[

#text(17pt)[
#grid(columns:2, gutter: 1em,
[
```rust
loop {
    for (client_id, client) in &mut CLIENTS {
        if client.offset < DATA.len() && !client.in_flight {
            let new_data = &DATA[client.offset..];
            let sqe = opcode::Send::new(
                client.conn.as_raw_fd(), new_data.as_ptr(), new_data.len() as u32,
            ).build();
            uring.submission().push(sqe.user_data(client_id));
            client.in_flight = true;
        }
    }
    uring.submit_and_wait(1)?;
    for cqe in uring.completion() {
        let client_id = cqe.user_data();
        let client = CLIENTS[client_id];
        let n = cqe.result()?;
        client.offset += n;
        client.in_flight = false;
    }
}
```
])
]]

== Performance

#pause

#let unknown = text(gray)[???]
#align(center,
table(columns:3, inset: 0.4em, stroke:none,
table.header([*Impl*], [*Clients*], [*Throughput*]),
table.hline(),
[\#1], [2.7k], [4 Gbps],
[\#2], [15k], [32 Gbps],
[\#3], [48k], [80 Gbps],
[\#4], [15k], [32 Gbps],
))

== Setup

```rust
let uring = IoUring::builder()
    .setup_single_issuer()
    .setup_coop_taskrun()
    .setup_defer_taskrun()
    .build(8<<10)?;
```

== Zero-copy

```rust
opcode::SendZc::new(
    fd,
    slice.as_ptr(),
    slice.len() as u32,
).build()
```

Produces two CQEs:
- one when data _enters_ the send queue
- one when data _leaves_ the send queue

== Registered files

```rust
opcode::SendZc::new(
    Fixed(1234), // <--- "fixed" == ring-local fd
    slice.as_ptr(),
    slice.len() as u32,
).build()
```

Ring-local fds, faster to use than regular (per-process) fds

```rust
uring.submitter().register_files_sparse(100_000)?;
```
```rust
uring.submitter().register_files_update(1234, &[some_fd])?;
```

== Registered buffers

```rust
opcode::SendZc::new(
    Fixed(1234),
    slice.as_ptr(),
    slice.len() as u32,
)
.buf_index(Some(0)) // <--- `slice` must be inside buf #0
.build();
```

Register regions of userspace memory; ops touching that memory run faster

---

```rust
uring.submitter().register_buffers(&[libc::iovec {
    iov_base: DATA.as_mut_ptr() as _,
    iov_len: DATA.capacity(),
}])?;
// And make sure DATA never gets moved!
```
#v(1fr)

== Implementation \#4.1

#text(17pt)[
```rust
static DATA: Mutex<Vec<u8>> = Mutex::new(Vec::with_capacity(16 << 30));
static CLIENTS: Mutex<Slab<Client>> = Mutex::new(Slab::new());
```

#grid(columns:2, gutter: 1em,
[
```rust
let sock = TcpListener::bind("0.0.0.0:80")?;
let mut upstream = connect_websocket("...")?;
let mut uring = IoUring::builder()....build(8<<10)?;
uring.submitter().register_files_sparse(100_000)?;
uring.submitter().register_buffers(...)?;
```
#titled-block(title: [`thread`])[
```rust
loop {
    let msg = upstream.next()?;
    let msg = add_ws_framing(msg);
    DATA.extend(&msg)?;
    assert!(DATA.len() <= 16 << 30);
}
```
]],
titled-block(title: [`thread`])[
```rust
loop {
    let (conn, _) = sock.accept()?;
    thread::spawn(move {
        accept_websocket(&conn)?;
        assert!(client_id < 100_000);
        uring.submitter().register_files_update(client_id, &[conn.as_raw_fd()]);
        CLIENTS.insert(Client {
            offset: DATA.len(),
            in_flight: false,
        });
    });
}
```
],
)
]

#slide[

#text(17pt)[
#grid(columns:2, gutter: 1em,
[
```rust
loop {
    for (client_id, client) in CLIENTS.iter_mut() {
        if client.offset < DATA.len() && !client.in_flight {
            let new_data = &DATA[client.offset..];
            let sqe = opcode::SendZc::new(
                Fixed(client_id),
                new_data.as_ptr(),
                new_data.len() as u32,
            )
            .buf_index(Some(0))
            .build();
            uring.submission().push(sqe.user_data(client_id));
            client.in_flight = true;
        }
    }
    uring.submit_and_wait(1)?;
    for cqe in uring.completion() {
        if cqueue::notif(cqe.flags()) { continue; }
        let client_id = cqe.user_data();
        let client = CLIENTS[client_id];
        let n = cqe.result()?;
        client.offset += n;
        client.in_flight = false;
    }
}
```
])
]]





// #text(17pt)[

// #grid(columns:2, gutter: 1em,
// [
// ```rust
// let sock = TcpListener::bind("0.0.0.0:80")?;
// let mut upstream = connect_websocket("...")?;
// let mut uring = IoUring::builder()....build(8<<10)?;
// static DATA: Mutex<Vec<u8>> = Mutex::new(Vec::with_capacity(16 << 30));
// static NOTIFY: Notify = Notify::const_new();

// const CODE_ACCEPT = 1u64 << 32;
// let op = opcode::AcceptMulti::new(sock.as_raw_fd()).build();
// unsafe {
//     uring.submission().push(&op.user_data(CODE_ACCEPT))?;
// }

// let mut clients = Slab::<Client>::default();

// ```


// #titled-block(title: [`thread`])[
// ```rust
// loop {
//     let msg = upstream.next()?;
//     data.extend(ws_header(&msg))?;
//     data.extend(&msg)?;
//     NOTIFY.notify_waiters();
// }
// ```
// ]],
// titled-block(title: [`task`])[
// ```rust
// loop {
//     let (conn, _) = sock.accept().await?;
//     tokio::spawn(async move {
//         let ws = accept_async(conn).await?;
//         let mut conn = ws.into_inner();
//         let mut offset = 0;
//         loop {
//             NOTIFY.notified().await;
//             conn.writable().await?;
//             sendfile(&conn, &*MEMFD,
//                      Some(&mut offset), n)?;
//         }
//     });
// }
// ```
// ])
// ]


== Performance

---

#v(1em)
#align(center)[
#text(red.darken(30%))[
```
Error: Cannot allocate memory
```
]]
#v(1em)

#pause

- `ulimit -l`
- `LimitMEMLOCK`
- `rlimit::setrlimit(Resource::MEMLOCK)`

```console
systemd-run --user -p LimitNOFILE=100000 -p LimitMEMLOCK=infinity
```
---

#let unknown = text(gray)[???]
#align(center,
table(columns:3, inset: 0.4em, stroke:none,
table.header([*Impl*], [*Clients*], [*Throughput*]),
table.hline(),
[\#1], [2.7k], [4 Gbps],
[\#2], [15k], [32 Gbps],
[\#3], [48k], [80 Gbps],
[\#4.1], [70k], [147 Gbps],
))

== Possibilities

- Can have multiple rings
    - one-per-core is recommended
    - eg. an embedded DB crate could have its own private ring
    - ring-to-ring messaging
- submit and wait
    - ...for at least `n` CQEs
    - ...with a timeout
- per-ring fd table
- multi-shot accept
    - fds registered to global or local table
- multi-shot recv
    - buffers pulled from a pool

== Caveats

- Not portable to other unixes
    - or even to older Linux kernels!
- Buffer management
- Fairness
    - can starve part of the state machine



