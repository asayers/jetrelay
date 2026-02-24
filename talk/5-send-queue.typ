#import "@preview/touying:0.6.1": *
#import "util.typ": *
#import "@preview/cetz:0.4.2"

#let cetz-canvas = touying-reducer.with(reduce: cetz.canvas, cover: cetz.draw.hide.with(bounds: true))

= Writing to TCP sockets

== TCP send queue

#align(center)[
#image("send_queue.svg", width:60%)]

// Logically, a queue of bytes
// Typically a few MiB in capacity

#v(1em)

- `write()` pushes data to the back
- Data at the front get packetised & sent
- Data dropped from the front when ACK'd

== Fragments

#align(center)[
#image("send_queue_2.svg", width:60%)
Think "queue of `Arc<[u8]>`" or "queue of `Bytes`"
]

// --- 
// // #quote(block: true, attribution: [`mm/page_frag_cache.c`])[
// // An arbitrary-length arbitrary-offset area of memory which resides within a
// // 0 or higher order page.  Multiple fragments within that page are
// // individually refcounted, in the page's reference counter.
// // ]

// #grid(columns:(1fr, 1.2fr), gutter: 1em,
// [
// #titled-block(title: [`C`])[
// ```c
// struct page_frag {
//     // backing allocation
//     // (refcounted)
//     struct page *page;
//     __u16 offset;
//     __u16 size;
// };
// ```
// ]
// ],
// [
// #text(16pt)[
// ```text
//     Arc ptrs                   ┌─────────┐
//     ________________________ / │ Bytes 2 │
//    /                           └─────────┘
//   /          ┌───────────┐     |         |
//  |_________/ │  Bytes 1  │     |         |
//  |           └───────────┘     |         |
//  |           |           | ___/ data     | tail
//  |      data |      tail |/              |
//  v           v           v               v
//  ┌─────┬─────┬───────────┬───────────────┬─────┐
//  │ Arc │     │           │               │     │
//  └─────┴─────┴───────────┴───────────────┴─────┘
// ```]
// ])

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
