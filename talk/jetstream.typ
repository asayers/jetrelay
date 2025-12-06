#import "@preview/touying:0.6.1": *
#import "util.typ": *
#import "@preview/fletcher:0.5.8" as fletcher
#import "@preview/tdtr:0.4.0": *

= The firehose

== Bluesky

Like Twitter, but data publically available \
⇒  you can look up someone's tweets

#speaker-note[
- Lookup someone's handle and see all their tweets
- The "ground truth" data
- Data lives on your server
- Like a blog with an RSS feed
]

#pause

#speaker-note[
- with RSS you periodically poll all your subscriptions to check
  for updates
- with microblogging you typically follow more people and want updates quickly
- so: having "everyone polling everyone" doesn't work
- have one beefy server poll _everyone_, generate a stream of updates
- people follow that stream (and filter)
- they call it "the firehose"
- You connect
- bombards you with all the new tweets
- in real-time
- Actually more than that:
- any time the state of the network changes in any way
- it sends an update
]
Also: "the firehose"

#pause
#speaker-note[
- So let's take a look at it
- Here's one of their servers
- Let's see if we can get the server to talk to us
- `ws:` scheme means it wants to speak the "websockets" protocol
]
#v(1fr)
#align(center)[
`ws://jetstream2.us-west.bsky.network/subscribe`

#small[("jetstream": A variant of the firehose; omits signatures etc.)]
]
#v(1fr)


== Websockets

// First thing we have to do is a handshake.
#titled-block(title: [Handshake])[
// Starts out looking like we're talking HTTP.
```
GET /subscribe HTTP/1.1
```

// Immediately ask the server to switch protocols.
 ```
Connection: Upgrade
Upgrade: websocket
```

// Now send some websockets-specific bits
```
Sec-WebSocket-Version: 13
Sec-WebSocket-Key: MTIzNDU2Nzg5MDEyMzQ1Ngo=
```
]

// The key is just 16 random bytes
// After this, the server starts sending data.


---

// Now the response
#titled-block(title: [Handshake (response)])[
```
HTTP/1.1 101 Switching Protocols
```

```
Connection: Upgrade
Upgrade: websocket
```

```
Server: jetstream
Sec-WebSocket-Accept: UzQo2NzMDEyM1NggMTIzND5=
```
]

// The "accept" string is a hash of the key we sent


// Then boom, we start receiving data

#slide[
#text(size:10pt)[
```json
<81>~^A<F6>{"did":"did:plc:5rg4sbiiiesfe563zzqumetx","time_us":1763689821448290,"kind":"commit","commit":{"rev":"3m6466ot3d72m","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466ostj72m","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:21.386Z","subject":{"cid":"bafyreiatvsmobksfekpb677zhwjkzbxgpawwpluzeduuglzyq2wpyjk64a","uri":"at://did:plc:o7ozygoaxci2djvrazhdmgds/app.bsky.feed.post/3m6466aia7s2u"}},"cid":"bafyreigyuqrq7phuhl5tzyf2ek5pgeea7p4gtsrmymfb7r3a4surjmf5qi"}}
```
```json
<81>~^A<F6>{"did":"did:plc:htmhzec2p7aru5juw43732am","time_us":1763689821448774,"kind":"commit","commit":{"rev":"3m6466ob3d72p","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466oaomx2p","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.741Z","subject":{"cid":"bafyreidyg66ngsfvzp5xgzw6vde2okbhker54k4bndx7ilz5rrdv545rk4","uri":"at://did:plc:upogmzimjos5wbp4y5dkzt7x/app.bsky.feed.post/3m5zzb6nms22o"}},"cid":"bafyreiftxgsfr7revfoafmrbuqks3ec5pkh2zfvl6uyzdohys76yhtdxnq"}}
```
```json
<81>~^C7{"did":"did:plc:nutrikdii7qo6xbb52mdlyf2","time_us":1763689821449193,"kind":"commit","commit":{"rev":"3m6466nzecl2z","operation":"create","collection":"app.bsky.feed.post","rkey":"3m64666fhis2e","record":{"$type":"app.bsky.feed.post","createdAt":"2025-11-21T01:50:04.166Z","embed":{"$type":"app.bsky.embed.images","images":[{"alt":"","aspectRatio":{"height":1500,"width":2000},"image":{"$type":"blob","ref":{"$link":"bafkreifnxbxn4vlpsdawnxktr7aoeyzocooux3b2uwxy3p7avs7npubiam"},"mimeType":"image/jpeg","size":886994}},{"alt":"","aspectRatio":{"height":1500,"width":2000},"image":{"$type":"blob","ref":{"$link":"bafkreid4toh755jum2mh64ds3ehon77fom3y3qaamhr2ywnfhg4dibwkiu"},"mimeType":"image/jpeg","size":970465}}]},"langs":["ja"],"text":"乗った！"},"cid":"bafyreiecicaisnrex4akqpyxzunn2sfhl6dl6y3l42drko4ab4zhj6otxu"}}
```
```json
<81>~^A<8A>{"did":"did:plc:hyhxd432dqwxzrurdymoj2b5","time_us":1763689821450390,"kind":"commit","commit":{"rev":"3m6466ny6f72r","operation":"create","collection":"app.bsky.graph.follow","rkey":"3m6466nxnrx2r","record":{"$type":"app.bsky.graph.follow","createdAt":"2025-11-21T01:50:20.390Z","subject":"did:plc:iwqdab3ifz6ooighwhslpiiz"},"cid":"bafyreia3zmpvp2gig7brfcdqg7lkgoi6t4jbbxmd6ynr2rosndedpbkjme"}}
```
```json
<81>~^A<FA>{"did":"did:plc:bgickkrqnnv7nyfh6ylbkxwb","time_us":1763689821450842,"kind":"commit","commit":{"rev":"3m6466ooca626","operation":"create","collection":"app.bsky.feed.repost","rkey":"3m6466ontlg26","record":{"$type":"app.bsky.feed.repost","createdAt":"2025-11-21T01:50:21.111Z","subject":{"cid":"bafyreiezmx2hskvynr3ru775zy6hitmrjj27ow43ic4sx6vy53m22zjhki","uri":"at://did:plc:t5g4e7slz7mc56xlbcpobdvf/app.bsky.feed.post/3m63av4qtae2m"}},"cid":"bafyreihee3wkujchuavh3xbqesl2w67rrxjrq2h2hyohz4pmg26qtfnpbe"}}
```
```json
<81>~^A<F6>{"did":"did:plc:cp4mjijntnhh27pdtqi7hucg","time_us":1763689821451503,"kind":"commit","commit":{"rev":"3m6466oom4a2j","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466oo4ia2j","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.931Z","subject":{"cid":"bafyreidynbpfmymrtfwcj6uax7pmmq7y35mv4ifc6kpaifvh7fayvr2mty","uri":"at://did:plc:pbtde2womj2b6sbmt2usuzyw/app.bsky.feed.post/3m63rrmcryk2e"}},"cid":"bafyreiehlccixoy34nc3mmcgxp5qndn6ullvlofmruwh75aiebwkgecwi4"}}
```
```json
<81>~^B<93>{"did":"did:plc:5segm472sboj3bznepzusiot","time_us":1763689821452124,"kind":"commit","commit":{"rev":"3m6466ogwke2b","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466ogiuu2b","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.918Z","subject":{"cid":"bafyreia32lc3lf6mdsdyuxx6e555sbjto2c5baegj5owg4cmmvcliicudq","uri":"at://did:plc:lrw4cnptopspeg5tthztcliw/app.bsky.feed.post/3m63g7tcnva2e"},"via":{"cid":"bafyreie6ofr46v6h2zuwud5gv3zlf2pvgs5s7a3jivn6ik3sa5kjghvxd4","uri":"at://did:plc:rrfwruhud4ovela3oe6isre5/app.bsky.feed.repost/3m644ru253n25"}},"cid":"bafyreiggz52lgkwhyuo4wnf5qrdyldwqvmufunzcuvwrsxp4glchcgz4fa"}}
```
]]

// Very small, but I think you can just abount make out that it's
// JSON objects, separated by some non-ASCII stuff

// Let's zoom in on one
// Someone is _creating_ a _like_...
// So this is what gets generated every time you click the like button!

#slide[
#text(size:16pt)[
```json
<81>~^A<F6>
{
  "did": "did:plc:5rg4sbiiiesfe563zzqumetx",
  "time_us": 1763689821448290,
  "kind": "commit",
  "commit": {
    "rev": "3m6466ot3d72m",
    "operation": "create",
    "collection": "app.bsky.feed.like",
    "rkey": "3m6466ostj72m",
    "record": {
      "$type": "app.bsky.feed.like",
      "createdAt": "2025-11-21T01:50:21.386Z",
      "subject": {
        "cid": "bafyreiatvsmobksfekpb677zhwjkzbxgpawwpluzeduuglzyq2wpyjk64a",
        "uri": "at://did:plc:o7ozygoaxci2djvrazhdmgds/app.bsky.feed.post/3m6466aia7s2u"
      }
    },
    "cid": "bafyreigyuqrq7phuhl5tzyf2ek5pgeea7p4gtsrmymfb7r3a4surjmf5qi"
  }
}
```
]
]

// These bytes at the start are the websocket frame header.
// Basically it encodes the length of the frame as a varint.

// Notice this timestamp field
// microseconds since the epoch
// it increases monotonically
// means it goes up and up as you go through the stream
//
// very useful
// that means you can use it as a index
// to refer to a certain position in the stream


== Handling disconnects

// And actually the server lets you specify,
// when you connect, which timestamp to start from

#titled-block(title: [Handshake])[
```
GET /subscribe?cursor=1763689821448290 HTTP/1.1
...
```
]

#pause

Remember the last timestamp you saw \
⇒  you can resume from where you were

#underline[Essential functionality]

== Relays

Data is distributed to clients via a network of relays

#let tree = it => align(center, tidy-tree-graph(
  text-size: 25pt,
  node-inset: 10pt,
  spacing: (30pt, 50pt),
  draw-node: ((name, label, pos)) => 
    (
      pos: (pos.x, pos.i),
      label: [#label],
      name: name,
      shape: fletcher.shapes.pill,
    ),
  it
))

// Fan-out architecture

// Horizontal scalability: crank up the number of servers to handle more clients
// Geographical distribution: have a relay in Europe which all your
// European clients connect to, avoid having lots of copies of the event stream
// crossing the Atlantic

// Today, we're going to build one of these:

#alternatives(
tree[
  - Firehose
    - Relay
      - Client
      - Client
    - Relay
      - Client
      - Client
    - Relay
      - Client
      - Client
],
tree[
  - Firehose
    - Relay
      - Client
      - Client
    - #text(red)[Relay]
      - Client
      - Client
    - Relay
      - Client
      - Client
],
)

#speaker-note[
Some of these relays transform the data too, eg.
the "jetstream" format I mentioned.  Ours is just
going to re-transmit the event data verbatim;
adding transformations is easy if needed however.
]

== How much data?

\~400 events/second

Typical event: \~0.5 KiB

Bitrate: \~200 KiB/s

#pause

// == Napkin maths

// Easy to find servers with network cards that can do 10 gigabits
10Gbps NIC => \~6000 simulteneous clients

// Amazon will rent you a machine with a 200 gigabit interface!
200Gbps NIC => \~120k simulteneous clients!

#speaker-note[
Clearly the amount of data
currently flowing through the network
is no problem for modern hardware
but what about the software...?
]

#pause

...but official implementation can only do 1 Gbps 😞

#speaker-note[
We can do better than that

(Note: the official server is feature-rich and the code is clearly
designed for flexibility, not performance.  We're going to go the
other way though: no features, high performance.)

Let's go!
]

