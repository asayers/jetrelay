#import "@preview/touying:0.6.1": *
#import "util.typ": *

= What about ... ?

== What about page cache fragmentation?

#align(center, image("ownership.svg", width: 70%))
---
#v(1.5mm)
#align(center, image("ownership_2.svg", width: 70%))


== What about UDP multicast?

#speaker-note[
We want to send a copy of each messages to all connected clients.
This situation is actually pretty common.
It's called "multicast"

The classical solution to the problem is a technology called "UDP multicast"

The way it works is:
there's a block of IP addresses called the "multicast range"
these IPs can't be assigned to individual hosts
rather, you use them to refer to "multicast groups"
Clients "subscribe" to a group by connecting to its IP address
Servers "publish" to a group by sending to the IP address
The message will be recieved by all connected clients
Simple!

TODO: Back when IP addresses were grouped by category...

And it's all taken care of by the network hardware itself
so it's super efficient
]

// ```rust
// use rustix::net::*;
// let multicast_addr = Ipv4Addr::new(233, 38, 231, 92);
// let sock = socket(AddressFamily::INET, SocketType::DGRAM, Some(ipproto::UDP))?;
// sockopt::set_ip_add_membership(&sock, multicast_addr, &Ipv4Addr::UNSPECIFIED)?;
// ```

// You can tell it's multicast if the first octet is in 224..240
// This is the address NPT uses to distribute clock updates

```rust
use socket2::*;
let multicast_addr = Ipv4Addr::new(224, 0, 1, 1);
let sock = Socket::new(
    Domain::IPV4,
    Type::DGRAM,
    Some(Protocol::UDP),
)?;
sock.join_multicast_v4(
    multicast_addr,
    &Ipv4Addr::UNSPECIFIED,
)?;
```

#speaker-note[
However, there are some caveats

It's UDP, so transmission can fail.  This means clients need
    - some way to detect dropped packets (a seq num) and
    - some way to re-request those packets (via a second, TCP-based, protocol)

(there are experiments and proposals for reliable multicast protocols, but none caught on)

There's a limit on how big your datagrams can be (depends on the network)
Again, this is just a UDP thing

Finally, this is the big one:
using UDP multicast requires some measure of trust
This means that it's strictly LAN-only
You can't do UDP multicast over the internet

]

== Caveats

- Manual reordering
- Manual re-requesting
- Message size limits
- LAN-only

#speaker-note[
But if everyone is on the same network then
it works really well, and it's used a lot.
eg. Stock exchanges use UDP multicast to distribute market data to the participants


Anyway...
the transport of jetstream isn't up for negotiation:
it's defined as websockets, which means TCP, and that's that (if we want to be compatible)
so UDP multicast is not going to help us
]

*Going even further...*

L1 fanout

== What about TLS?

- don't encrypt
- ktls
    - hardware offload
- nginx
