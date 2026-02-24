#import "@preview/touying:0.6.1": *
#import "util.typ": *
#import "@preview/cetz:0.4.2"

#let cetz-canvas = touying-reducer.with(reduce: cetz.canvas, cover: cetz.draw.hide.with(bounds: true))

= Writing to TCP sockets

== TCP send queue

Logically, a queue of bytes

Typically a few MiB in capacity

`write()`ing to the socket pushes to the back

Bytes at the front get packetised & sent

Bytes dropped from the front when ACK'd

== Fragments

// #quote(block: true, attribution: [`mm/page_frag_cache.c`])[
// An arbitrary-length arbitrary-offset area of memory which resides within a
// 0 or higher order page.  Multiple fragments within that page are
// individually refcounted, in the page's reference counter.
// ]

#grid(columns:(1fr, 1.2fr), gutter: 1em,
[
#titled-block(title: [`C`])[
```c
struct page_frag {
    // backing allocation
    // (refcounted)
    struct page *page;
    __u16 offset;
    __u16 size;
};
```
]
],
[
#text(16pt)[
```text
    Arc ptrs                   ┌─────────┐
    ________________________ / │ Bytes 2 │
   /                           └─────────┘
  /          ┌───────────┐     |         |
 |_________/ │  Bytes 1  │     |         |
 |           └───────────┘     |         |
 |           |           | ___/ data     | tail
 |      data |      tail |/              |
 v           v           v               v
 ┌─────┬─────┬───────────┬───────────────┬─────┐
 │ Arc │     │           │               │     │
 └─────┴─────┴───────────┴───────────────┴─────┘
```]
])

// If you're ever used the "bytes" crate this may look familiar

// #small[There's also an optimization which allows very small fragments to be
// inlined into the skb]

== `write()`

// Notes from andrew:

// - Add some pseudocode to the diagram slides
// - He thought the "many writes" slide meant a big chunk of the vec was being copied, not multiple copies
//     - suggests a vertical frag cache, or maybe scattered?  (But it's a bump arena...)

#align(center, image("zerocopy_1.svg", height: 80%))

== `write() write() write()`
#speaker-note[
    Now we send the same data to multiple clients, which each have their own
    send queue...
]
#align(center, image("zerocopy_2.svg", height: 80%))
#speaker-note[...but look at all these copies!]

---

#align(center, image("zerocopy_3.svg", height: 80%))
#speaker-note[
    _This_ is what we want - one copy, lots of references to the same data.  But
    the problem is there's no way to refer to _this_ (the fragment) when calling
    `write()`.
]

= Zero-copy

== What we want is...

A buffer

...owned by the kernel

...that will survive across multiple `write()`s

#speaker-note[

    What could this "handle" possibly be?
    Well, in Linux, handles are known as "file descriptors".
    and indeed, we have something called...
]

#pause

#v(1cm)
...a *memfd*

== memfd

#table(
columns: (0.7fr, 1fr, 1fr), inset: 0.5em,
table.header([],
[*Vec\<u8\>*],
[*memfd*],
),
[Create],
[`Vec::new()`],
[`memfd_create()`],
[Append],
[`extend()`],
[`write()`],
// [Modify],
// [`copy_from_slice()`],
// [`pwrite()`],
[Modify],
[`&mut xs[..]`],
[`mmap()`],
[Resize],
[`resize()`],
[`ftruncate()`],
// [Free],
// [`mem::drop()`],
// [`close()`],
[Punch hole],
text(size:20pt, fill:gray)[N/A],
[`fallocate()`],
)

#speaker-note[
    You can push bytes on the end, causing it to grow
    You can modify existing bytes
    You can resize it (fills in with zeroes if you enlarge it)
    
    Do note however that all of these are syscalls!
    So they're more expensive to call
    than these
    But that's shouganai if we're modifying memory owned by the kernel

    Plus there's a bonus operation: hole-punching
    not supported by Vecs
    well... you can do it with `madvise()`...
]

---

// Implementation:\
Just a file which doesn't do writeback

Data lives in the page cache

Same as creating a file in /tmp \
#small[...more or less, assuming tmpfs and `O_TMPFILE`]

#speaker-note[
    ...and what is this thing?
    Just a file!
    The contents live in the page cache, like any other file.
    The only difference is that 
    writeback is turned off.
    So the pages remain permanently dirty.
    Because there's nowhere for it to
    write back to.
    (Except swap of course)
]

#speaker-note[
    And you'll notice that all these functions are
    the same ones you'd use on a normal file,
    and they work the exact same way.
]


#speaker-note[
    By the way, a memfd is basically the same thing as opening a file on a tmpfs
    with `O_TMPFILE`
]

#speaker-note[
    Create it with `memfd_create()`.  This allocates an inode in the VFS, puts
    an entry in your process's file table, and returns the fd.
]

// #speaker-note[
// And when I say "kernel-owned" (you could quibble and say "all memory is owned by
// the kernel") I mean "owned by the page cache"
// ]

// Zero-filled regions don't consume any memory \
// Memory pages are allocated as you write to it \
// (`Vec::with_capacity(huge)` has the same property)

== memfd

```rust
let file = memfd_create("firehose", MemfdFlags::CLOEXEC)?;
```

#pause

#v(1fr)

```console
$ ls /proc/272127/fd
0  1  2  3
```

#speaker-note[
These are all symlinks, so let's see where they point
]

#pause

#v(1fr)

```console
$ ls -l /proc/272127/fd
lrwx------ - asayers 30 Dec 16:05 0 -> /dev/pts/10
lrwx------ - asayers 30 Dec 16:05 1 -> /dev/pts/10
lrwx------ - asayers 30 Dec 16:05 2 -> /dev/pts/10
lrwx------ - asayers 30 Dec 16:05 3 -> '/memfd:firehose (deleted)'
```

#speaker-note[
stdin, stdout, stderr are linked to terminal 10, that makes sense
But fd3, the memfd, is a symlink pointing to... /memfd:...???
That file doesn't exist!

Turns out these aren't really symlinks at all...
like everything in /proc and /sys, these "files" are just
part of the kernel-userspace interface.
The kernel wants to show some handy info about these fds,
so it smuggles that info in the "symlink target" field.

This has got to be one of the craziest user interfaces I've ever seen
]

#v(1fr)

== sendfile()

#speaker-note[
Take a slice of `file` and push it onto `sock`'s send queue
]

#align(center, image("zerocopy_4.svg", height: 70%))

#align(center)[
```rust
sendfile(sock, file, offset, len)
```
]

== Implementation \#3

#text(16pt)[

#grid(columns:2, gutter: 1em,
[
```rust
let sock = TcpListener::bind("0.0.0.0:80").await?;
let (mut upstream, _) = connect_async("...").await?;
static DATA: LazyLock<File> = LazyLock::new(||
    memfd_create("firehose", ...),
);
static NOTIFY: Notify = Notify::const_new();
```

#titled-block(title: [`task`])[
```rust
loop {
    let msg = upstream.next().await?;
    let msg = add_ws_framing(msg);
    DATA.write_all(&msg)?;
    NOTIFY.notify_waiters();
}
```
]],
titled-block(title: [`task`])[
```rust
loop {
    let (conn, _) = sock.accept().await?;
    tokio::spawn(async move {
        accept_async(&conn).await?;
        let mut offset = DATA.metadata()?.len();
        loop {
            NOTIFY.notified().await;
            conn.async_io(Interest::WRITABLE, || {
                rustix::fs::sendfile(
                    &conn, &*DATA,
                    Some(&mut offset), 2 << 20,
                )
            }).await?;
        }
    });
}
```
])

]

== Performance

#let unknown = text(gray)[???]
#align(center,
table(columns:3, inset: 0.4em, stroke:none,
table.header([*Impl*], [*Clients*], [*Throughput*]),
table.hline(),
[\#1], [2.7k], [4 Gbps],
[\#2], [15k], [32 Gbps],
[\#3], [48k], [80 Gbps], // 30k => <1s
[\#4], unknown, unknown,
))

#speaker-note[
By the way, we could have just used a regular file instead of a memfd.
It would all work the exact same way
except that the data gets sync'd to disk after a while
]

== Caveats

- Portability
- Memory locked in page cache
- Modifying in-flight data

== Zero-copy options

- `sendfile()`
- `splice()` and `tee()`
- `vmsplice()`
- `MSG_ZEROCOPY`

#speaker-note[
By the way, if you recall the diagram from earlier,
I showed ref-counted slices of pagecache-owned data being placed in the send queue.
So within the kernel the data is being passed around and stored "by reference".
But you should know it's actually passed by reference _all the way to the NIC_.
The kernel will pass these slices to the NIC,
and the NIC will deference them, and read bytes directly out of RAM and onto the wire.
So it really is zero copy!
]

/*

== ???

#table(columns:4,
[],               [mapped],[disk-backed],[can be `sendfile()`'d],
[`Vec::new()`     ],[ ✅ ],[  ❌  ],[ ❌ #small[(memory owned by PTEs, not page cache)] ],
[`File::open()`   ],[ ❌ ],[ ✅   ],[ ✅ #small[(memory owned by page cache, has inode etc.)] ],
[`mmap(file)`     ],[ ✅ ],[  ✅  ],[ ✅ #small[(memory owned by page cache, has inode etc.)] ],
[`memfd_create()` ],[ ❌ ],[ ❌   ],[ ✅ #small[(memory owned by page cache, has inode etc.)] ],
)
*/
