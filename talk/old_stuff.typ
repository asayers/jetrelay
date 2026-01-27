

== Page allocator [OLD]

#speaker-note[
- Let's talk about how the kernel allocates memory
- Start with memory divided into max-size folios and all marked "free"
- Kernel has an allocator for physical memory
- It starts out with all memory in one big "free" chunk
- Request allocation =>
  - splits it in half
  - splits that in half
  - again until the region is of size requested
- You can only ask for power-of-two sized allocations
- There's also a min/max allocation size:
  - 4 KiB (base page)
  - 2 MiB (huge page)
]

#cetz-canvas(length: 25cm / 16, {
  import cetz.draw: *
  let height = 0.5;
  rect((0,0), (4,height))
  rect((4,0), (8,height))
  rect((8,0), (12,height))
  rect((12,0), (16,height))
  (pause,)
  rect(fill: red, (8,0), (12,height))
  (pause,)
  rect(fill: red, (8,0), (10,height))
  rect(fill: white, (10,0), (12,height))
  (pause,)
  rect(fill: red, (8,0), (9,height))
  rect(fill: white, (9,0), (10,height))
})

#pause

You can split a block into two smaller blocks \
You can merge a block with its "buddy" to form a bigger block

=> an allocation's size is always a power-of-two

Min size: 4 KiB #small[(typically)] \
Max size: 4 MiB #small[(typically)]

// This is called a "buddy allocator" and it's pretty common
// eg. jemalloc uses this technique
// (but with much more fine-grained size classes)
//
// You can refer to an allocation by its base address and size
// If you have 2^n base pages, you can do this in n+1 bits
// This follows from the fact that a binary
// tree has fewer inner nodes than leaves,
// so with one extra bit you can number
// all the inner nodes as well as the leaves.
//
// Most allocations are a single page
// Each CPU keeps a pool of pre-allocated pages ready to be used (PCP - "per-CPU pages")
// so you can avoid hitting the buddy allocator (and taking a lock)
//
// For sub-page allocations, there's
// - SLAB (size classes, free lists)
// - page_frag_cache (bump arena, per-CPU, no global allocator)
//
// Ignored: Zones, migrate types

== Folios [OLD]

Allocation
\+ refcount // ...except frozen folios
#small[\+ `flags` + `lru` + `mapping`/`index` + ...]

...actually two refcounts:
- kernelspace
- userspace #small[(called the "mapcount")]

Refcount goes to zero => add to freelist and merge up
// When you free a page, you check to see if its buddy
// is also free
// if it is, you can merge them into a 8 KiB free region
// and etc. cascading merges up the tree

If refcount > 1 => COW (shared)

// ---

// // Represents a contiguous set of bytes
// //
// // A folio is a physically, virtually and logically contiguous set
// // of bytes.  It is a power-of-two in size, and it is aligned to that
// // same power-of-two.  It is at least as large as %PAGE_SIZE.  If it is
// // in the page cache, it is at a file offset which is a multiple of that
// // power-of-two.  It may be mapped into userspace at an address which is
// // at an arbitrary page offset, but its kernel virtual address is aligned
// // to its size.
// #show raw: it => text(13pt, it)

// ```c
// struct folio {
//     memdesc_flags_t flags; // Identical to the page flags.
//     // Least Recently Used list; tracks how recently this folio was used.
//     struct list_head lru;
//     // The file this page belongs to, or refers to the anon_vma for anonymous memory.
//     struct address_space *mapping;
//     // Offset within the file, in units of pages.  For anonymous memory,
//     // this is the index from the beginning of the mmap.
//     pgoff_t index;
//     atomic_t _mapcount; // how many times this folio is mapped by userspace.
//     atomic_t _refcount; // how many references there are to this folio.
//     unsigned int _nr_pages; // Do not use directly, call folio_nr_pages().
//     // Folios to be split under memory pressure.
//     struct list_head _deferred_list;
// };
// ```
#speaker-note[
- There are other allocators built on top of this
- But folios are the fundamental unit of allocation
]

== Fragments [OLD]


#speaker-note[
- Pages are the fundamental unit of memory allocation
- But for actually moving data around you use "fragments"
- a slice of a folio
- bumps the refcount on the underlying folio
- If you're ever used the "bytes" crate this may look familiar
]

```c
struct page_frag {
    struct folio *folio;
    __u16 offset;
    __u16 size;
};
```
// ```c
// struct page_frag {
//     // Points to folio metadata, including refcount
//     struct page *page;
//     __u16 offset;
//     __u16 size;
// };
// ```

// ```rust
// struct Fragment {
//     folio: *Folio, // Contains the refcount
//     folio: Arc<[u8]>,
//     offset: u16,
//     len: u16
// }
// ```
// Creating a page frag increments the ref count on the underlying page

Used everywhere, eg.

```rust
type Pipe = (VecDeque<Fragment>, WaitQueue);
```

== Writing to a TCP socket [OLD]

Every TCP connection has a "send queue"
// Double-ended queue of data which needs to be sent (or resent)

Write to socket ⇒  push data onto the back \
Ack arrives ⇒  discard data from the front

#pause
```rust
type SendQueue = LinkedList<Fragment>
```


// Simplified slightly:
// There's also an optimisation for very small bits of data (headers etc.)
// which lets you store those inline in the queue
// But most data is represented by page frags


// There's a page frac allocator \
// Per-CPU page used like a bump arena \
// Gets a new page from buddy allocator when full

// eg.
// write() to a TCP connection
// => get the (per-CPU) page
//    (alloc a new one if not enough room)
//    push data onto page
//    create a page fragment
//    push fragment onto back of send queue
// 

// ```rust
// type SendQueue = LinkedList<Skb>;
// struct Skb {
//     headers: SmallVec<u8, 256>,
//     payload: SmallVec<Fragment, 32>,
// }
// ```
// #small[(rough sizes)]

#v(1fr)
#pause
When you call `write()`,
the kernel needs to \
get your data into
a (kernel-owned) page
#v(1fr)
// It grabs a spare page, or allocates a new one if necessary,
// and copies your data into it
// (using it like a mini bump arena)
// and then pushes a page frag onto the back of the socket's send queue

// We do this for every client
// But what if the kernel could push the _same_ page frag to every client's
// send queue?

== A file's page cache [OLD]

Represented by a data-structure called an `xarray`.

Divides the file into 4 KiB chunks \
Maps the chunks to pages of memory (refcounted)

```rust
HashMap<ChunkIdx, Arc<[u8; 4096]>>
```

Source of truth for a file's contents.  Whenever you interact with a file,
you're interacting with its page cache

// ...unless you opened it in "direct mode"

// #slide[
//   = A file's page cache

//   A sparse cache of the file's contents

//   Represented by a datastructure called an `xarray`. \
//   Maps regions of the file to chunks of memory ("folios")

//   - Regions must have length 2#super[n] for some n ≥ 12 \
//   - Regions must be aligned to a multiple of its length

//   Whenever you interact with a file, \
//   you're really interacting with its page cache

//   // ...unless you opened it in "direct mode"
// ]

== sendfile [OLD]

```
sendfile(sock, file, offset, len)
```

Go to the file's page cache, look up the desired range

=> list of page slices

// Data will be resitend in the page cache, because we just wrote it

Go to the socket's send queue, push slices onto the back

That's it.


