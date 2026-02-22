#import "@preview/touying:0.6.1": *
#import "util.typ": *

= Conclusion

== Recap

#speaker-note[
...and that's as far as we'll be going down the rabbit hole today.
Of course there are always ways to go further
but the next step would probably be kernel bypass,
which requires non-standard hardware,
so I'm drawing the line there.
]

- Non-blocking I/O // via epoll/tokio
- Batching
- Zero-copy // via sendfile, but you could also use send_zc
- Full async I/O // via io_uring

== Where to go from here?

- More threads, more rings
- Kernel bypass?
- Custom hardware...

== Conclusions

- A single box can serve a lot of clients!
    - if you milk that box for all it's worth
- Writing platform-agnostic code is noble
    - but going platform-specific unlocks many cool features
    - if you're writing software in-house, you probably know _exactly_ where it will be run
- Relinquishing ownership of your data over to the kernel can be useful
    - you can get it back again with `mmap()`, if you need to

