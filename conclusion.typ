


- A single box can serve a lot of clients!
  - if you milk that box for all it's worth
- Writing platform-agnostic code is noble
  - but going platform-specific unlocks many cool features
  - if you're writing software in-house, you probably know _exactly_ where it will be run
- Relinquishing ownership of your data over to the kernel can be useful
  - you can get it back again with `mmap()`, if you need to






refcount > 1 ⇒ COW
  - It's a little more complicated - there are multiple refcounts and sometimes a page can be considered safe to
    mutate even when refcount >1



what about TLS?
  - don't encrypt
  - ktls
  - hardware offload
  - nginx





                 | mapped | disk-backed | can be `sendfile()`'d
`Vec::new()`     | ✔      | ❌          | ❌ (memory is owned by PTEs, not page cache)
`File::open()`   | ❌     | ✔           | ✔  (memory is owned by page cache, has inode etc.)
`mmap(file)`     | ✔      | ✔           | ✔  (memory is owned by page cache, has inode etc.)
`memfd_create()` | ❌     | ❌          | ✔  (memory is owned by page cache, has inode etc.)



            | Multiple of 4 KiB   | Use pages directly
Short-lived | Variable-sized      | Bump arena
Long-lived  | Fixed-sized         | Slab
                                  | vmalloc?
Long-lived  | Variable-sized      | kmalloc?





```rust
// One `struct page` for each page of physical memory
static mut MEMMAP: [Page; N_PAGES];
struct Pfn(u32); // Index into MEMMAP

// Now (64 bytes)
enum Page {
    FolioHead(Folio),
    FolioTail(Pfn /* of the head */),
    Slab(Slab),
    // ...
    Free {
        order: u8,
        prev: Pfn,
        next: Pfn,
    }
}

// Future (8 bytes)
enum Page {
    // many consequtive MEMMAP slots may point to the same folio
    File(Box<Folio>),
    Anon(Box<Folio>),
    Slab, // metadata inlined into the allocation
    // ...
    Free { // still inline
        order: u8,
        prev: Pfn,
        next: Pfn,
    }
}

struct Folio {
    flags: u32,
    lru_prev: Pfn,
    lru_next: Pfn,
    mapping: Mapping, // The "owner" of this folio
    index: u32 /* pages */, // The index into `mapping` which will take you back here
    private: *void,
    refcount: AtomicU16,
    mapcount: AtomicU16,
    pincount: AtomicU16,
    order: u8,
    pfn: Pfn,
    memcg_data: u32, // Box<MemCG>?
}
enum Mapping {
    File(Box<AddressSpace>),
    Anon(Box<AnonVma>),
}

struct Slab {
    flags: u32,
    next: *Slab,
    union {
        slab_list_prev: *Slab;
        struct {    // Partial pages
            slabs: i16,    // Nr of slabs left
            pobjects: i16, // Approximate count
        };
        rcu_callback: *void,
    },
    slab_cache: *kmem_cache,
    freelist: *void, // first free object
    s_mem: *void, // first object
    active: u16,
    refcount: AtomicU16;
    memcg_data: u32,
};
struct Slub {
    flags: u32,
    next: *Slub,
    union {
        slab_list_prev: *Slub;
        struct {    // Partial pages
            slabs: i16,    // Nr of slabs left
            pobjects: i16, // Approximate count
        };
        rcu_callback: *void,
    },
    slab_cache: *kmem_cache,
    freelist: *void,    /* first free object */
    union {
        counters: u32,    /* SLUB */
        struct {    /* SLUB */
            inuse: u16,
            objects: u15,
            frozen: bool,
        };
    };
    refcount: AtomicU16;
    memcg_data: u32,
};
struct Slob {
    flags: u32,
    next: *Slob,
    union {
        slab_list_prev: *Slob;
        struct {    // Partial pages
            slabs: i16,    // Nr of slabs left
            pobjects: i16, // Approximate count
        };
        rcu_callback: *void,
    },
    freelist: *void,    /* first free object */
    units: i16,
    refcount: AtomicU16;
    memcg_data: u32,
};

impl Page {
    fn order(self) -> u8;
    fn head(self) -> Pfn;
}
```

- Future: it'll just be a typed pointer ("memdesc") to a metadata struct (eg. `struct folio`, `struct slab`, ...)
- Now: the metadata is inline in 
