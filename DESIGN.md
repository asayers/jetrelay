<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Jetrelay</title>
<style>
body { max-width: 800px; margin: auto; padding: 1em; text-align: justify; }
h1 { text-align: center; }
pre { margin-left: 3em; }
.footnote-definition p { display: inline; }
.footnote-definition { padding: 1em; }
table { border-collapse: collapse; margin: auto; }
td, th { border-right: solid black 1px; border-left: solid black 1px; padding: 0.4em }
th { border-bottom: solid black 1px; }
td:first-of-type, td:last-of-type, th:first-of-type, th:last-of-type { border-left:none; border-right:none; }
td { text-align: right; }
summary h2,h3 { display: inline; }
.appendix { padding-top: 1em; padding-bottom: 1em; }
img { display: block; margin: auto; }
figure { margin-bottom: 2em; }
</style>

Let the kernel do the work!<br>Tricks for implementing a pub/sub server
=======================================================================

---

This post explains the design of **jetrelay**, a pub/sub server compatible
with Bluesky's "jetstream" data feed.  Its performance is an order of
magnitude better than the official jetstream server.  Read on to learn what
(Linux-specific) tricks it has up its sleeve.

> tricks it has up its sleeve.
This reads weird to me: it sounds like it is about the future, whereas you
already says the perf is an order of magnitude better.  I would use "how it
accomplishes this" or swap the sentences somehow.

---

## ATproto, jetstream, and relays

Bluesky is built on ATproto, and a core part of ATproto is "the firehose", a
stream of events representing all changes to the state of the network.  The
firehose contains all the new posts, as you'd expect; but also people liking
things, deleting/editing their old posts, following people, etc.  It covers the
whole of Bluesky, so it's fairly active.

> Bluesky
> ATproto
Consider adding links.

This data comes in two flavours: the original full-fat firehose, and a new
slimmed-down version called "[jetstream](https://docs.bsky.app/blog/jetstream)".
Both feeds are websockets-based, but jetstream encodes its payloads as JSON
(rather than CBOR) and omits the bits that are only needed for authentication.
Also, I think jetstream only contains a subset of the events.

The average message size on jetstream is around half a kilobyte.  The event rate
is variable (I guess it depends on which countries are awake), but it seems to
be around 300--500 events per second.  A _relay_ is a server which follows an
upstream feed provider and re-broadcasts the data to its own clients.[^relay topo]
Napkin estimate: running on a machine with a 10 gigabit NIC, your relay
should be able serve `10Gbps / (0.5KiB * 400/s)` = [~6000 clients][numbat]
simultaneously.

[numbat]: https://numbat.dev/?q=10+Gbps+%2F+%280.5+KiB+*+400%2Fs%29%E2%8F%8E

OK, challenge accepted!  I've written a simple jetstream relay which I'm calling
"jetrelay".  [The code lives here](https://github.com/asayers/jetrelay), and
in this post I'm going to explain how it works.  It's only ~500 LOC, and very
little of it is actually specific to jetstream.  The techniques described below
should be applicable to any pub/sub protocol.

> any pub/sub protocol.
maybe add some example, e.g. XMPP

<details>
<summary>There are some differences between jetrelay and the official jetstream
server.  Click here to see them.</summary>

* The official jetstream server consumes the full-fat firehose as its upstream
  data source, but for simplicity jetrelay will use another jetstream server as
  its upstream. JSON in, JSON out.
* When clients connect to the server they specify their initial position in the
  feed (via a timestamp).  This allows clients to backfill any data they may have
  missed.  Jetrelay will support this, since I do think it counts as "essential
  functionality".

> Jetrelay will support this
"This feature is currently missing but will be added in a future release"?

* The official jetstream server lets clients filter the data by
  [collection](https://atproto.com/guides/glossary#collection) or by
  [DID](https://atproto.com/specs/did).  This is clearly a central feature of
  jetstream, but jetrelay is going to omit it for now.  We're going to focus on
  the "full stream" use-case: every client gets the complete feed, whether they
  want it or not.
* Finally, the official server is cross-platform.  Jetrelay is Linux-only.

</details>

## Trick #1: Multicast, and bypassing userspace with `sendfile()`

Our remit is to accept events from an upstream data feed and re-broadcast those
events to our clients.  The key observation is that we're sending the _exact
same data_ to all clients. And I don't just mean the JSON values are the same;
the header bytes in the websocket frames are the same too.  Once the initial
handshake is complete, the bytes one client sees coming down the pipe are
identical to what any other client sees.

> identical
My main question when reading this is how this works with encryption.  Does the
protocol not support it?  Maybe worth adding that.

This is called "multicast".  On local networks, you can use UDP
multicast[^multicast group] and have the kernel/network hardware take care of
everything for you (although it's UDP so there are some gotchas[^udp caveat 1]
[^udp caveat 2]).  The jetstream protocol is based on websockets, though, which
is based on TCP.  Multicast-for-TCP isn't really a thing,[^reliable multicast]
so we're going to have to implement it ourselves.

The vast majority of the time, the data we'll be sending out is events which
have just come in. _Sometimes_, though, we'll need to send old events.  This
happens when a client connects asking to be back-filled from a certain point in
the feed.  It can also happen when clients are just slow to read.  This means
a copy of all the data we receive will need to be written to disk.

OK, time for our first neat trick!  As new events arrive, we'll append them
to a file.  We'll store the data exactly as it'll look on the wire---websocket
framing bytes and everything---all ready to go.  For each client, we keep a
cursor which points to some position in the file.  If a client's cursor doesn't
point to the end of the file, we copy the missing bytes from the file into the
client's socket.

The kernel has a syscall for this: `sendfile()`.  You specify a file, a range
of bytes within the file, and a socket to send the bytes to.  Not only is this
very simple, it has great performance.  You might think "fetching data from
disk sounds expensive", but since this is data we've just written, it will be
resident in the kernel's page cache (ie. in memory).  And with `sendfile()`, the
data goes straight from the page cache to the network stack, without needing to
be copied into our program's memory in between.

> it has great performance
it is a very cheap operation

The best thing about this design is that it naturally batches writes for clients
which are a long way behind.  A client which is up-to-date will receive new
messages as soon as they're ready; but if there are multiple messages ready to
send, they can all be copied into the socket as a single chunk.  If the chunk is
bigger than 4 KiB (~8 events) then the kernel can avoid even more copies, since
a complete pages of data can simply be passed around by reference.

Smoothly trading away latency in favour of throughput when clients are falling
behind is really important for this kind of application.  Programs which do
the opposite---get less efficient when under load---are the stuff of SRE horror
stories.


## Trick #2: Handling many clients in parallel with `io_uring`

One syscall, no copies---what more could you want!  Well, `sendfile()` blocks
the current thread until the copy is complete.  If the client is slow, they can
block us for a long time.  In order to avoid starving fast clients, we'd have
to spawn a thread per-client.  That's no good---we're trying to scale to many
thousands of clients here.

> That's no good
It's often claimed that threads are cheap (I don't know about rust specifically)
but maybe worth adding that while they're not expensive, they're still not free.
Maybe include a napkin estimate of memory per thread x6000 clients.

So enter the second piece of high-tech: `io_uring`.  With this we can issue a
bunch of `sendfile()`s---one per client---and then submit them all to the kernel
in a single syscall.  Our main runloop will look like this:

> the second piece of high-tech
the second cool Linux feature?

1. For each client: if it's behind (and writeable), add a `sendfile()` to the
   submission queue.
2. Submit the I/Os and wait for completions (with a timeout).
3. For each completed `sendfile()`: bump the client's cursor.
4. Go to (1).

So all-in-all our program will have three long-lived threads:

* **event writer thread**: This one follows the upstream ATproto event feed and
  writes the new events to the file (including the websocket header).
* **client handshake thread**: This one listens for new client connections.
  When new a client connects, it performs the HTTP/websocket handshake and adds
  them to the set of active clients.
* **main runloop thread**: This is the one (just described above) which tries to
  keep clients up-to-date with the file.

When clients are far behind, we'll loop quickly to get them caught up.  When the
clients are all up-to-date, we'll re-check the file length periodically (based
on the timeout).

With this design, neither the number of threads nor the number of syscalls
depends on the number of clients. A huge number of clients can be connected,
and the amount of (userspace) work our program does barely changes.  Of course,
the amount of work the _kernel_ has to do does increase---but there's no getting
around that.  Our job is to orchestrate the necessary I/O as efficiently as
possible and then get out of the kernel's way.

One detail I glossed over: io_uring doesn't actually have a sendfile operation!
But not to fear: we can emulate a `sendfile()` with two `splice()`s. First you
splice from the file to a pipe, then from the pipe to the socket.  (This is
actually how synchronous `sendfile()` is implemented within the kernel.)  The
two splice operations can be submitted at the same time; you submit them as
"linked" entries, which means io_uring won't start the second splice until the
first one has completed.  You need to give each client its own pipe.

Thanks to the awesome [rustix crate](https://github.com/bytecodealliance/rustix)
for making implementing all this stuff easy![^rustix]


<details>
<summary><h3>Diagrams</h3></summary>

I'm told that the above explanation needs some diagrams, so here's my best shot
at making some:

<figure>

![new event arrives](new_event.svg)

<figcaption>

A new event arrives from upstream and is copied to the end of the file.  The
clients' cursors were previously pointing to the end of the file, but now there's data in front of their
cursors which needs to be sent.

</figcaption>
</figure>

<figure>

![submit I/O](submit_io.svg)

<figcaption>

Jetrelay submits a bunch of `splice()`s to the kernel via an io_uring submission
queue.  These instruct the kernel to splice the new data from the file into the
clients' sockets.

</figcaption>
</figure>

<figure>

![receive an I/O completion](receive_completion.svg)

<figcaption>

We receive a completion for one of the `splice()`s.  The relevant client's
cursor is moved to its new position.  Client 2 has received the new data and is
now up-to-date.

</figcaption>
</figure>

I hope that helps!

</details>

<!-- ## Storing client state -->

<!-- For each client we need to maintain the following state: -->

<!-- * its current offset in the file (mutable) -->
<!-- * the fd of its socket -->
<!-- * the fds of its pipe (used to pull off the "double splice" trick above). -->

<!-- When we submit a pair of splice operations for a client, we attach a cookie -->
<!-- which identifies which client those operations were for.  When the operations -->
<!-- complete, we get a notification containing that cookie.  At this point we can -->
<!-- bump the offset of the relevant client. -->

<!-- Every time the file grows, we need to iterate over all the clients.  When we're -->
<!-- notified that a write has completed, we need to bump the client's offset.  This -->
<!-- means we need to store the client state in a way which allows fast iteration and -->
<!-- fast point access.  Something like a `Vec<ClientState>` would do the trick. -->

<!-- However, this is meant to be a long-running process, and clients can come and -->
<!-- go, so we do need to take care to clean up after disconnected clients. You could -->
<!-- use a `SlotMap`, which is similar to `Vec<Option<ClientState>>`, except it uses -->
<!-- the empty slots to maintain a freelist, similar to a memory allocator.  I'm just -->
<!-- using a `BTreeMap<ClientId, ClientState>` because it's simple and fast enough. -->


## Not a trick: Setting the initial cursor with an index

When clients connect they can specify their initial position, as a timestamp.
To support this we're going to need and index which translates timestamps to
byte-offsets in our file.  I'm using a `BTreeMap<Timestamp, Offset>`.[^btreemap]

Remember that we have an "event-writer" thread which receives ATproto events
from upstream and writes them to the file?  We can give that thread a bit more
work to do:  for each event, it parses the timestamp, and pushes an entry to the
index. The index needn't cover _every_ event, so we could do this only every _N_
events; but it's fast enough, so I just do it every time.

Now, suppose a client connects, requesting data from a certain timestamp
onwards.  The "client handshake" thread looks up the timestamp in the index.
The returned value is their initial byte offset.  (Since this index is being
accessed by multiple threads, I've shoved it in a `Mutex`.[^mutex])

After this, everything Just Works!  To the main runloop, this client just looks
like it somehow got a long way behind, and it starts `sendfile()`ing data to
it as fast as it can.  As discussed above, this can send 4 KiB chunks extremely
efficiently, so clients get caught up _fast_.

Note that this index stuff really only has to be _good enough_.  The
event-writer thread needs to be able to keep up with the feed, and that's it.
The client-handshake thread needs to respond within 100ms.  Not hard.  We could
get fancy and keep our index in a lock-free deque... but why make life hard for
ourselves, eh?  `Mutex<BTreeMap<_, _>>` does the job.

This section is labelled "not a trick"; but you might say the trick is
recognising when performance is unimportant.

<!-- There are basically three tasks our program is going to perform: -->

<!-- 1. Accept new client connections and do some handshaking/initialization. -->
<!-- 2. Receive messages from upstream, process them, and write them to disk. -->
<!-- 3. Send message data from disk to clients. -->

<!-- Let's discuss performance constraints. -->

<!-- 1. Initializing clients should happen in some _reasonable_ amount of time. -->
   <!-- Under 100ms, let's say.  Beyond that, there's no point in optimizing it. -->
<!-- 2. We need to be able to keep up with the messages coming from upstream.  It -->
   <!-- should have a bit of headroom to allow for growth in ATproto usage.  But -->
   <!-- beyond that there's no point optimizing this: if we can handle 200x what -->
   <!-- upstream is throwing at us, that's cool, but it doesn't actually buy us -->
   <!-- anything. -->
<!-- 3. Sending data to clients---this is the one that matters.  We're going to push -->
   <!-- this metric as far as we possibly can.  The more efficiently we can do this, -->
   <!-- the more clients can connect at the same time before they start falling -->
   <!-- behind.  This translates directly to saved hardware costs. -->

<!-- Criteria (1) and (2) only need to be _satisfied_.  Once they're ticked off, all -->
<!-- our effort is going to go into making (3) as fast as we possibly can. -->


## Trick #3: Discarding old data with `FALLOC_FL_PUNCH_HOLE`

The final problem we need to solve is discarding old data. With our current
implementation, the file will grow and grow until we run out of disk space. I'd
like our implementation to keep data for a fixed amount of time before deleting
it. It doesn't have to happen continuously---periodic cleanup operations are
fine---but it should happen without disturbing connected clients.

At this point you're probably thinking about rotating the file.  This would
introduce additional bookkeeping and tricky corner cases (eg. when clients cross
the boundary from one file to the next).  Does it have to be this complicated?

No! Once again the OS provides us with a simple solution: `fallocate()`. This
syscall allows us to deallocate regions of a file.  (Linux refers to this as
"punching a hole" in the file.)  Deallocated regions take up no disk space, and
return zeroes when you read from them.

The rest of the data in the file remains readable at the offset as before. This
means there's no need to fix-up client cursors or anything: to the rest of the
program it's as if nothing ever happened (so long as it never reads from the
deallocated region).

So, when we want to remove data older than a given time, we first remove all
entries from the index up to that time, then we take the offset of the last
entry we removed, and deallocate the file up to that point.  (This is why I
used a `BTreeMap` to store the index in the previous section, rather than a
`HashMap`: btrees support fast range deletion.)

Now, if you look at the file with `ls`, you'll see its length just goes up and
up.  But if you look at it with `du`, you'll see the amount of disk space it's
actually using remains bounded!

```console
$ ls -l
213M jetrelay.dat
$ du -h
8.9M jetrelay.dat
```

You might be thinking: if the apparent size of the file grows and grows, surely
we'll hit some kind of file size limit eventually? Well, the max file size
depends on your configuration, but it's generally measured in petabytes.  So we
can stream terabytes of data per month for hundreds of years before this becomes
a problem.  As clever hacks go, I'd say this one has a relatively long shelf
life!

One edge-case to take care of:  if a client doesn't read from its socket, its
cursor doesn't move.  So it's possible that, when we go to discard some old
data, there are clients whose cursors still point into the region we want to
deallocate.  We should kick such clients off at this point, or else they would
see zeroes if they suddenly started reading again.  New clients will never end
up in the deallocated region, even if they request an ancient timestamp, because
we removed those entries from the index.

## Testing it out

### ...against loopback

<!-- ### How does it perform? -->

<!-- Connecting directly to the official jetstream instance, I could only connect -->
<!-- 10 clients before they would start to fall behind.  But with a jetrelay instance -->
<!-- sitting in front of it, I was able to connect 11 _thousand_ clients no problem. -->

First, let's take it for a spin locally.  I'm running jetrelay on my laptop and
connecting lots of clients to it.  The clients will connect to jetrelay over the
loopback interface; this means no actual network traffic will hit my NIC (which
is good since I'm on wi-fi).  Ok, let's go...

```
Error: Too many open files (os error 24)
```

...right.  Each client consumes a file descriptor for its socket, and two more
for the pipe we're using to implement `sendfile()`.  By default, a program can
only have 1024 file descriptors at a time---that's only 340 clients!  We're
going to need to raise the fd limit.  The traditional way to do this is using
`ulimit -n`, but since I'm running jetrelay with systemd anyway I can just set
`LimitNOFILE=65535` in the unit file.[^fd limit]

With that fixed, I connected 20k clients to jetrelay, and they were having no
problem keeping up with the feed. The total throughput was over 24 Gbps.  Nice!

<!-- ### Jettester -->

<!-- I wrote a program called "jettester" which opens thousands of connections to a -->
<!-- jetstream server and reports the worst latency.  (This also needs to be run with -->
<!-- an increased fd limit.) -->

### ...against a real network

<!-- The number I wanted to measure for each server is this: how many clients can -->
<!-- connect before some clients start falling behind?  A client can look at the -->
<!-- timestamp of the latest message it received and see far behind the current time -->
<!-- that is.  A client is "falling behind" when that distance looks like it's growing -->
<!-- worse and worse, with no prospect of ever catching up. -->

Loopback is all well and good, but I want to see it working for real, over an
actual network.  So I rented a VM from Linode.  I went for the cheapest instance
type with a 10 Gbps outgoing network connection.

From a different machine in the same datacenter, I opened 6000 simultaneous
client connections.  All 6000 kept up with the feed easily:

```
6000 clients @ T-0s (319 evs, 158 KiB)
Total: 1_914_000 evs, 930 MiB, 7440 Mbps
```


The jetstream feed happened to be fairly quiet when I was doing these tests
(only around 300 events/second), so even with 6k clients the datarate was only
7.4 Gbps.

I kept increasing the number of clients.  At 8.5k, it was still keeping them all
fed.  Finally, at 9k, I exceeded the limit.  For a moment it was actually able
to keep all 9000 clients up-to-date:

```
9000 clients @ T-0s (279 evs, 141 KiB)
Total: 2_511_000 evs, 1245 MiB, 9960 Mbps
```

But very soon some of the clients started falling behind:

```
 130 clients @ T-1s (392 evs, 162 KiB)
8870 clients @ T-0s (269 evs, 144 KiB)
Total: 2_396_614 evs, 1255 MiB, 10040 Mbps
```

As expected, this happens just as the required datarate exceeds 10 Gbps.  And we
can see that jetrelay is indeed saturating the full 10 Gbps.

### Finding the limit

So, jetrelay can manage 10 Gbps's worth of clients---great.  But could it serve
_more_ with a beefier NIC?

Presumably the NIC was the bottleneck in the 9k client test; but on the other
hand, jetrelay wasn't exactly short of processing power: this machine has
50 CPU cores! (Yes, this was the _cheapest_ one I could get with 10-gigabit
networking.)

I want to find the point at which the output becomes bottlenecked by jetrelay,
not the NIC.  I can't increase the network bandwidth... but I _can_ reduce the
number of CPU cores!  My strategy will be to gradually reduce jetrelay's CPU
quota until it can no longer manage 10 Gbps.  (I'll do this using `systemctl
set-property jetrelay.service CPUQuota=`.)

Here are the results:

CPU quota | Max clients | Throughput
----------|-------------|------------
50 CPUs   | 8.5k        | 9.7 Gbps
 9 CPUs   | 8.5k        | 9.7 Gbps
 8 CPUs   | 8.0k        | 9.1 Gbps
 7 CPUs   | 7.8k        | 8.3 Gbps
 6 CPUs   | 6.6k        | 6.8 Gbps
 5 CPUs   | 5.4k        | 5.8 Gbps

...aaand I've used up my monthly transfer quota.

The throughput would fluctuate from second to second.  These figures are the
average over roughly a minute.  I think it was still bottlenecked on networking
at 9 CPUs.  With 8 cores or fewer it's clearly bottlenecked on CPU.


### How does it compare to the official server?

The official jetstream server is architecturally very different to jetrelay.
Each client has an associated goroutine which performs per-client filtering.
There are many channels involved.  I think this is all quite conventional for
golang programs.

Another difference is that event data is stored in an LSM-tree (using "pebble",
an embedded database by the authors of CockroachDB).  Again, this was probably
done with the filtering use-case in mind.  However, I suspect it lowers the
performance ceiling somewhat.

We're stress-testing the "no-filtering" case, while the official server is
clearly optimized for the "lots-of-filtering" case. This is going to be a tough
test for it.

So anyway, how does it do?  The server contains some per-IP rate-limiting logic,
so in order to stress-test it I first had to [surgically remove that
code](https://github.com/asayers/jetstream/commit/97d168146849bde567c185f3ca65fa408dfb8fca).

With that done I went about testing it in the same way as above.  However, I
wasn't able to get it to exceed 2 Gbps, even with all 50 cores available; and
the typical throughput was closer to 1 Gbps.  I tried turning up the worker
count and per-client rate, but didn't observe any difference.

I did notice something odd however: clients weren't falling behind.  Despite
the low throughput, all clients continued to observe very recent events.  What's
going on?

It turns out that when the server is overloaded, it starts skipping events! I'm
not sure whether this is a bug or deliberate behaviour.  Interestingly, it looks
like all clients drop the _same_ events.

So... I'm not exactly sure what the maximum throughput of the official server
is.  If this event-dropping is due to a bug, then perhaps it could get more
than 2 Gbps with the bug fixed.  I was really trying to compare these relays
in terms the throughput their _architectures_ can support, not the specific
implementations.

<!-- Libraries like Pebble or RocksDB are trying to be applicable to lots of problems -->
<!-- and useable on lots of platforms.  This is what makes them so useful; but it's -->
<!-- also a serious handicap!  Pebble might be brilliantly engineered, but it's -->
<!-- not _specifically designed_ to back a jetstream relay. -->

## Wrap-up

If you design a system geared specifically for the problem you have and the
platform you're running on, the sky (or your NIC) is the limit!

<!-- This was my first time writing a blog post.  It's a surprising amount of work! -->
<!-- Thanks to Jasper van der Jeught and Francesco Mazzolli for giving me some tips. -->


---

<details class="appendix">
<summary><h2>Appendix: The other 90%</h2></summary>

Jetrelay is a tech demo.  Here's a non-exhaustive list of things you'd want
before running it For Real:

Operational stuff:

* Backfill on startup (jetrelay starts with an empty file)
* Be a better websocket citizen (respond to pings, send close frames, etc.)
* Notify systemd when ready to receive connections
* Better logging etc.
* Prometheus metrics
* The official jetstream server has a fancy dashboard

Security stuff (maybe nginx can provide some of this?):

* Per-IP rate limiting
* Preventing DoS by spamming new connections
* Timing out clients who take a long time to do the handshake
* Preventing clients from sending a large amount of data

Missing features:

* Filtering by collection.
  I imagine this would work by writing the events into multiple
  files - one per collection.  Then we interleave the contents of those files when
  sending them to clients.  We'll need to take care only to interleave at valid
  frame boundaries.
* Filtering by DID.
  This is going to require a different approach, since it's not
  feasible to have a file per-DID.  I guess an in-memory index, and then selective
  `sendfile()`ing.
* In-band stream control

Last but not least: more testing, including fuzzing.  Most of this stuff should
be no trouble to add; it just takes work.

In a real deployment you'd probably want to run jetrelay behind nginx, for
virtual hosts and TLS and such.  Nginx can't handle the number of connections
and volume of traffic that jetrelay can, so in this kind of setup, nginx is
going to be the bottleneck.

</details>

<details class="appendix">
<summary><h2>Appendix: Thoughts on ATproto and the "push-based internet"</h2></summary>

The OG internet is pull-based: a client requests a resource, and the server
gives it to them.  But in some situations you want things to be sent to you
without having to ask.  The classic example is email: if you only have pull,
then you're forced to periodically ask the server "anything new?";  but ideally
the server should push a "new email" notification to you unprompted (at which
point you do a pull to get the contents).

The problem with "push" is the number of TCP connection you need to keep open.
You need an open connection to every server you want receive notifications from,
which doesn't scale well.  Maintaining a persistent connection to your email
provider is one thing; but consider microblogging: if the people you follow
span a hundred different servers, you'll need to maintain a hundred long-lived
TCP connections---ouch!

So unlike pull, push requires a middleman to be practical.  This middleman
aggregates all the notifications and then fans them out---now you only need to
subscribe to the middleman!  Think of Apple's push notification system: your
phone connects to one of Apple's servers, and then that server relays push
notifications on behalf of WhatsApp, Uber, etc.

Another comparison you could make is with RSS.  The way RSS works is that each
publisher has a list of entries ("feed.xml"), and they're allowed to add entries
to the top of the list. Feed aggregators poll these lists periodically, and when
a new entry appears they push a notification to subscribers.  Very sensible.

ATproto is similar!  But it makes some improvements. First: the data structure.
RSS gives you an append-only list.  ATproto gives you a map from paths to
records. This means that, as well as creating new entries, you can also edit
or delete old ones.  What you end up with is a filesystem-like tree of files...
just like HTTP![^atproto paths]  The fact that the model matches the OG internet
means you can expose the same data over HTTP for people who'd rather pull.
Giving each record an identity (in the form of a path) is also just very useful
in itself.

The second improvement over RSS is that ATproto records are signed.  This makes
it impossible for the relay to attribute fake updates to people.  I've never
heard of RSS aggregators performing this kind of attack, but in theory it's
possible. Removing trust from the relay is nice because it means you can use
whatever relay is physically closest without worrying much about about who the
operator is.[^verify in theory]

The pull-based internet has HTTP.  The push-based internet has made do with RSS
for a long time.  A new, more capable standard could be a great development.

</details>

---

## Footnotes

[^relay topo]: By chaining relays together, you can quickly fan-out the feed to
  a large number of clients.  If you place your relays in strategic locations,
  you can also distribute the feed all around the world with minimal traffic.

[^multicast group]: You create a _multicast group_, which is a special kind of
  socket which you can add subscribers to, and any packet you send to the group
  socket is mirrored to all the subscribers.  It's very convenient!

[^udp caveat 1]: Messages have to fit within a certain size limit defined by the
  network.  If you go over 508 bytes, you might find the message gets dropped
  every time.

[^udp caveat 2]: Delivery is unreliable so clients need a way to re-request
  lost packets.

[^reliable multicast]: Although various [reliable
  multicast](https://en.wikipedia.org/wiki/Reliable_multicast) protocols do
  exist, none are very popular AFAIK.

[^rustix]: Rustix is a crate which provides Linux's userspace API in a nice
  rusty wrapper.  It's a bit like libc, re-imagined for rust (although in
  general it's a "thinner" wrapper than libc is).  It's more flexible than
  libstd, but just as user-friendly.  It's awesome!
   
[^btreemap]:  Why a `BTreeMap` and not a `HashMap`?  You'll find out in the
  next section!
   
[^mutex]: Why a `Mutex` and not a `RWLock`?  An `RWLock` effectively gives
  priority to readers---in this case, the "client handshake" thread---at the
  expense of writers---the event writer thread.  That's not what I want.

<!-- [^rpi]: Yes yes I know ARM doesn't have real mode. -->

[^fd limit]: Why use 2**16?  I don't actually know!  But this is the limit I
  always see people use.  Perhaps it's just a meme.

[^atproto paths]: ...except that paths use dots as the separator, instead of
  slashes... but not the final separator, that one _is_ a slash. 🤷

[^verify in theory]: And I think being able to verify the messages in theory is
  good enough.  Most clients won't actually bother doing it, but that's fine. So
  long as some clients are checking the relay (and so long as the relay doesn't
  know which clients are checking), any sneaky business risks getting caught.
  This could lead to reputational damage, or even financial damage if the relay
  operator has signed contracts with its users.
