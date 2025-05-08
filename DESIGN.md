<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Jetrelay</title>

<style>
body { max-width: 800px; margin: auto; padding: 1em; }
body { text-align: justify; hyphens: auto; }
header { margin-bottom: 2em; }
header h1 { text-align: center; font-size: 2em; }
pre { margin-left: 3em; }
code { font-family: Menlo, Monaco, Consolas, 'Lucida Console', monospace; font-size: 85%; margin: 0; hyphens: manual; }
table { border-collapse: collapse; margin: auto; }
td, th { border-right: solid black 1px; border-left: solid black 1px; padding: 0.4em }
th { border-bottom: solid black 1px; }
td:first-of-type, td:last-of-type, th:first-of-type, th:last-of-type { border-left:none; border-right:none; }
td { text-align: right; }
summary h2,h3 { display: inline; }
.appendix { padding-top: 1em; padding-bottom: 1em; }
img { display: block; margin: auto; }
figure { margin-bottom: 2em; }
date { display: block; text-align: right; font-style: italic; }
img { height: auto; max-width: 100%; }
</style>

</head>
<body>
<article>
<header>


Let the kernel do the work!<br>Tricks for implementing a pub/sub server
=======================================================================

---

This post explains the design of **jetrelay**, a pub/sub server compatible with
Bluesky's "jetstream" data feed.  Using a few pertinent Linux kernel features,
it avoids doing almost any work itself.  As a result, it's highly efficient: it
can saturate a 10 Gbps network connection with just 8 CPU cores.

---

<date>May 2025</date>
</header>

<!-- max-width: 36em; -->
<!-- padding: 50px; -->
<!-- h1, h2, h3, { margin-top: 1.4em; } -->
<!-- ol, ul { padding-left: 1.7em; margin-top: 1em; } -->
<!-- li > ol, li > ul { margin-top: 0; } -->
<!-- blockquote { margin: 1em 0 1em 1.7em; padding-left: 1em; border-left: 2px solid #e6e6e6; color: #606060; } -->
<!-- pre { margin: 1em 0; overflow: auto; } -->
<!-- pre code { padding: 0; overflow: visible; overflow-wrap: normal; } -->
<!-- table { margin: 1em 0; border-collapse: collapse; width: 100%; overflow-x: auto; display: block; } -->
<!-- table caption { margin-bottom: 0.75em; } -->
<!-- code{white-space: pre-wrap;} -->

The challenge: Broadcasting at line rate
----------------------------------------

Bluesky is built on ATproto, and a core part of ATproto is "[the firehose]",
a stream of events representing all changes to the state of the network.  The
firehose contains all the new posts, as you'd expect; but also people liking
things, deleting/editing their old posts, following people, etc.  It covers the
whole of Bluesky, so it's fairly active.

[the firehose]: https://atproto.com/specs/sync#firehose

This data comes in two flavours: the original full-fat firehose, and a new
slimmed-down version called "[jetstream]".
Both feeds are websockets-based, but jetstream encodes its payloads as JSON
(rather than CBOR) and omits the bits that are only needed for authentication.
Also, I think jetstream only contains a subset of the events.

[jetstream]: https://docs.bsky.app/blog/jetstream

The average message size on jetstream is around half a kilobyte.  The event
rate is variable (I guess it depends on which countries are awake), but it
seems to be around 300--500 events per second.  A _relay_ is a server which
follows an upstream feed provider and re-broadcasts the data to its own
clients.[^relay_topo] Napkin estimate: running on a machine with a 10 gigabit
NIC, your relay should be able serve [`10Gbps / (0.5KiB * 400/s)`][numbat] =
~6000 clients simultaneously.

[numbat]: https://numbat.dev/?q=10+Gbps+%2F+%280.5+KiB+*+400%2Fs%29%E2%8F%8E

OK, challenge accepted!  I've written a simple jetstream relay
which I'm calling "jetrelay".  It's only ~500 LOC, [the code lives
here](https://github.com/asayers/jetrelay), and in this post I'm going to
explain how it works.

Very few of those 500 lines are actually specific to jetstream.  The point
of jetrelay is to demonstrate the techniques described below, which should
be transferrable to implementations of other pub/sub protocols.  Be aware,
though, that jetrelay is a tech demo---[more code](#appendix-tech-demo) would be
required before using it in anger.

Also note: I'm not aiming for feature-parity with the official jetstream server.
In particular, the official server lets clients filter the data by [collection]
or by [DID].  I'm focusing just on the "full stream" use-case: every client
gets the complete feed, whether they want it or not.  Bluesky clearly considers
filtering to be an important feature, so I thought it worth mentioning the
omission.  There are some other[^json-in-json-out] differences[^compression]
too.

[collection]: https://atproto.com/guides/glossary#collection
[DID]: https://atproto.com/specs/did


Multicast and backfill
----------------------

Our remit is to accept events from an upstream data feed and re-broadcast those
events to our clients.  The key observation is that we're sending the _exact
same data_ to all clients.  And I don't just mean the JSON values are the
same; it's _all_ the same, right down to the headers of the websocket frames.
Excluding the initial HTTP handshake, every client sees the same bytes coming
down the pipe.[^encryption]

This is called "multicast".  On local networks, you can use UDP
multicast[^multicast_group] and have the kernel/network hardware take care of
everything for you (although it's UDP so there are some gotchas[^udp_caveat_1]
[^udp_caveat_2]).  The jetstream protocol is based on websockets, though, which
is based on TCP.  Multicast-for-TCP isn't really a thing,[^reliable_multicast]
so we're going to have to implement it ourselves.

A second observation is that, although clients do see the exact same events,
they _don't_ necessarily see them at the exact same time.  Fast clients will always
be receiving the latest events, but slower clients may start lagging behind when
the feed is especially  active.  These slow clients will be receiving a delayed version of
the feed until things quiet down and they get a chance to catch up.

And then there's backfill:  when clients connect to the server they specify
their initial position in the feed via a timestamp.  This allows clients to come
back online after a disconnect and fill in the events they missed.  In other
words, it's perfectly normal to be sending out data which is minutes or even
hours old.

The upshot is that our relay is not going to be a purely in-memory system. A
copy of all the event data will need to be saved to disk.

Trick #1: Bypassing userspace with `sendfile()`
-----------------------------------------------

As new events arrive, we'll append them to a file.  We'll store the data exactly
as it'll look on the wire---websocket framing bytes and everything---all ready
to go.[^kafka]  For each client, we keep a cursor which points to some position
in the file.  If a client's cursor doesn't point to the end of the file, we copy
the missing bytes from the file into the client's socket.  ...And that's it!

The kernel has a syscall for this: `sendfile()`.  You specify a file, a range
of bytes within the file, and a socket to send the bytes to.  Not only is this
easy-to-use, it's also very cheap.  You might think "fetching data from disk
sounds expensive"; but since this is data we've only just written, it will be
resident in the kernel's page cache (ie. in memory).  And with `sendfile()`, the
data goes straight from the page cache to the network stack.  Compare this with
a conventional `read()`/`write()` approach, where the data would be copied into
our program's memory and then back again.

<figure>

![](sendfile.svg)

<figcaption>

A new event arrives from upstream and is written to the end of the file.  The
clients are no longer considered "up-to-date", because their cursors no longer
point to the end of the file.  For each client, we call `sendfile()` to send the
new data, updating the client's cursor when the call returns.

</figcaption>
</figure>

The _best_ thing about this "file-and-cursor" design:  it naturally performs
write-batching.  A client which is up-to-date will receive new messages
one-at-a-time, as soon as they're ready; but if there are multiple messages
ready to send, they can all be copied into the socket as a single chunk.  Better
yet, page-sized chunks of data (4 KiB = ~8 events) can be passed by reference.
In this case, the network stack is literally reading data straight out of the
page cache, with zero unnecessary copies!

Smoothly trading away latency in favour of throughput when clients are falling
behind is really important for this kind of application.  Programs which do
the opposite---ie. get _less_ efficient when under load---are the stuff of SRE horror
stories.


Trick #2: Handling many clients in parallel with io_uring
---------------------------------------------------------

One syscall, no copies---what more could you want!  Well, `sendfile()` is
synchronous: it blocks the current thread until the data is sent.  But if
a client has gone AWOL and its send buffer is full, the next `sendfile()`
to that client will block indefinitely!  That means we're gonna need to give
each client its own dedicated thread.  But I don't really want to spawn 6000
threads...[^threads]

Fortunately there's a better solution!  Linux has a mechanism called "io_uring".
With this we can prepare a bunch of `sendfile()`s and submit them all to the
kernel in a single syscall.  The kernel then sends back completion events as the
`sendfile()`s finish.  It's like a channel for syscalls!

With io_uring, our main runloop looks like this:

1. For each client which is not up-to-date (and is writeable), add a
   `sendfile()` to the submission queue.
2. Submit all the `sendfile()`s and wait for completions (with a timeout).
3. For each completion, update the associated client's cursor.
4. Go to (1).

When all clients are up-to-date, the thread will sleep, waking up periodically
to re-check the file length (thanks to the timeout).  On the other hand, if
a client is far behind and hungry for data, the thread will loop quickly,
submitting a new `sendfile()` as soon as the previous one completes, and the
client will get caught up fast.

Note: the number of syscalls performed by our program does not depend on the
number of clients!  A huge number of clients can connect, and the amount of work
jetrelay does will barely change.  Of course, the _kernel_ will have more work
to do---but that's unavoidable.  Our job is to orchestrate the necessary I/O as
efficiently as possible and then get out of the kernel's way.

One detail I glossed over: io_uring doesn't actually have a sendfile operation!
But not to fear: we can emulate a `sendfile()` with two `splice()`s. First you
splice from the file to a pipe, then from the pipe to the socket.  (This is
actually how synchronous `sendfile()` is implemented within the kernel.)  The
two splice operations can be submitted at the same time; you submit them as
"linked" entries, which means io_uring won't start the second splice until the
first one has completed.  You need to give each client its own pipe.

Thanks to the awesome [rustix crate](https://github.com/bytecodealliance/rustix)
for making implementing all this stuff easy![^rustix]


Not a trick: Getting new clients connected
------------------------------------------

In the first section I told you about the **event writer thread**, which
receives ATproto events from upstream and writes them to the file.  In the
second section I described the **I/O orchestration thread**, which keeps
clients up-to-date with the file.

Jetrelay has one more thread which I haven't mentioned yet: the **client
handshake thread**.  This one listens for new client connections.  When new a
client connects, it performs the HTTP/websocket handshake and adds them to the
set of active clients.

Recall that when clients connect they can specify their initial position, as
a timestamp.  To support this we're going to need and index which translates
timestamps to byte-offsets in our file.  I'm using a `BTreeMap<Timestamp,
Offset>`.[^btreemap]

We can give the event-writer thread a bit more work to do:  after it finishes
writing an event to the file, it parses the event's timestamp and pushes an
entry to the index.  In theory the index doesn't have to cover _every_ event,
so we could do this only every _N_ events;  but it's fast, so I just do it every
time.

Now, when a client connects, all we have to do is to look up the requested
timestamp in the index and use the result to initialise the client's file
offset.  Note that the index is being written by one thread and read by another,
so I've shoved it in a `Mutex`.

...and that's it!  The next time the io_uring thread wakes up it'll see the new
client and start `sendfile()`ing data to it as fast as it can.  As discussed
above, this can send 4 KiB chunks extremely efficiently, so clients get caught
up _fast_.

This section is labelled "not a trick"; but you might say the trick
is recognising when the easy way is good enough.  If it takes clients
100ms to connect, who cares?  If we speed it up to 10ms, that doesn't
buy us anything.[^connecting-in-a-loop]  We could get fancy and keep our
index in a lock-free deque... but why make life hard for ourselves, eh?
`Mutex<BTreeMap<Timestamp, Offset>>` does the job.

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


Trick #3: Discarding old data with `FALLOC_FL_PUNCH_HOLE`
---------------------------------------------------------

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


Testing it out
--------------

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
`LimitNOFILE=65535` in the unit file.[^fd_limit]

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
(only around 300 events/second), so even with 6k clients the data-rate was only
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

As expected, this happens just as the required data-rate exceeds 10 Gbps.  And we
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
Each client has an "outbox" which buffers outgoing events for that client. When
a new event arrives, it's immediately copied into the outboxes of all "live"
clients.  Each client has an associated goroutine which drains the client's
outbox into its socket (via `write()`).

Another difference is that event data is stored in an LSM-tree.  I assume this
is also done with the filtering use-case in mind.  However, I suspect it lowers
the performance ceiling somewhat.  The data is stored pre-serialized as JSON,
but the websocket headers are generated fresh every time an event is sent.

So anyway, how does it do?  The server contains some per-IP rate-limiting logic,
so in order to stress-test it I first had to [surgically remove that
code](https://github.com/asayers/jetstream/commit/97d168146849bde567c185f3ca65fa408dfb8fca).

With that done I went about testing it in the same way as above.  However, I
wasn't able to get it to exceed 2 Gbps, even with all 50 cores available; and
the typical throughput was closer to 1 Gbps.  I tried turning up the worker
count and per-client rate, but didn't observe any difference.

It should be noted that the official server is clearly optimized for the
"lots-of-filtering" case.  We're stress-testing the "no-filtering" case.  This
was always going to be a tough test for it.

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

This was my first time writing a blog post.  It's a surprising amount of work!
Thanks to [Jasper] and [Francesco] for giving me some tips.

[Jasper]: https://jaspervdj.be/
[Francesco]: https://mazzo.li/

<!-- Thanks to Jasper Van der Jeugt and Francesco Mazzoli for giving me some tips. -->


---

<details class="appendix">
<summary><h2 id="appendix-tech-demo">Appendix: The other 90%</h2></summary>

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
just like HTTP![^atproto_paths]  The fact that the model matches the OG internet
means you can expose the same data over HTTP for people who'd rather pull.
Giving each record an identity (in the form of a path) is also just very useful
in itself.

The second improvement over RSS is that ATproto records are signed.  This makes
it impossible for the relay to attribute fake updates to people.  In theory an
RSS aggregator could perform this kind of attack (though I've never heard of it
happening).  Note that the opposite attack, where the relay selectively drops
events, is still possible with ATproto.

Removing trust from the relay is nice because it means you can use
whatever relay is physically closest without worrying much about about who the
operator is.[^verify_in_theory]

The pull-based internet has HTTP.  The push-based internet has made do with RSS
for a long time.  A new, more capable standard could be a great development.

</details>

---

## Footnotes

[^relay_topo]: By chaining relays together, you can quickly fan-out the feed to
  a large number of clients.  If you place your relays in strategic locations,
  you can also distribute the feed all around the world with minimal traffic.

[^json-in-json-out]: The official jetstream server consumes the full-fat
  firehose as its upstream data source, but for simplicity jetrelay uses another
  jetstream server as its upstream.  JSON in, JSON out.

[^compression]: Another feature offered by the official jetstream server is
  a zstd-compressed version of the feed.  I didn't add support for this, but
  adding it would be trivial.

[^encryption]: You might be wondering, "what about encryption?"  For
  TLS-encrypted (`wss://`) websockets, the bits on the wire _aren't_ the same
  for all clients.  (If they were, it wouldn't be a very secure encryption
  method!)  So, this trick does't work for encrypted streams.<br>
  However!  A server like jetrelay wouldn't normally support TLS natively
  anyway.  That's because you'd typically be running it behind a reverse proxy
  (nginx or similar), to support virtual hosts and the like; and at that point
  you might as well let nginx take care of TLS for you.  So, whether the sockets
  lead directly to our clients or to nginx, our job is to get unencrypted
  jetstream data into those sockets as fast as possible.

[^multicast_group]: You create a _multicast group_, which is a special kind of
  socket which you can add subscribers to, and any packet you send to the group
  socket is mirrored to all the subscribers.  It's very convenient!

[^udp_caveat_1]: Messages have to fit within a certain size limit defined by the
  network.  If you go over 508 bytes, you might find the message gets dropped
  every time.

[^udp_caveat_2]: Delivery is unreliable so clients need a way to re-request
  lost packets.

[^reliable_multicast]: Although various [reliable
  multicast](https://en.wikipedia.org/wiki/Reliable_multicast) protocols do
  exist, none are very popular AFAIK.

[^kafka]: This is a trick I learned from Kafka.

[^threads]: A thread-per-client architecture _could_ be made to work at this
  scale: 6000 threads is not crazy.  Each thread allocates 8MiB of stack space,
  but that's not a problem: this memory isn't actually committed until written
  to.  The main problem is pressure on the scheduler.  With thousands of threads
  constantly waking up and going to sleep, the rest of the system is going to
  become quite unresponsive.  I'm sure this is a solvable issue, with a bit of
  cgroups wizardry...  but I didn't explore far enough to know.  io_uring was
  just easier than using lots of threads.

[^rustix]: Rustix is a crate which provides Linux's userspace API in a nice
  rusty wrapper.  It's a bit like libc, re-imagined for rust (although in
  general it's a "thinner" wrapper than libc is).  It's more flexible than
  libstd, but just as user-friendly.  It's awesome!
   
[^btreemap]:  Why a `BTreeMap` and not a `HashMap`?  You'll find out in the
  next section!

[^connecting-in-a-loop]: For clients, connecting is a one-time thing.  Unless
  they're connecting, reading an event or two, then disconnecting, in a tight
  loop...  but that's not behaviour I want to encourage!

[^fd_limit]: Why use 2**16?  I don't actually know!  But this is the limit I
  always see people use.  Perhaps it's just a meme.

[^atproto_paths]: ...except that paths use dots as the separator, instead of
  slashes... but not the final separator, that one _is_ a slash. 🤷

[^verify_in_theory]: And I think being able to verify the messages in theory is
  good enough.  Most clients won't actually bother doing it, but that's fine. So
  long as some clients are checking the relay (and so long as the relay doesn't
  know which clients are checking), any sneaky business risks getting caught.
  This could lead to reputational damage, or even financial damage if the relay
  operator has signed contracts with its users.

</article>
</body>
</html>
