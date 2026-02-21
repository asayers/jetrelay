#import "@preview/touying:0.6.1": *
#import "util.typ": *
#import "@preview/fletcher:0.5.8" as fletcher
#import "@preview/tdtr:0.4.0": *

= Bluesky

/*
== Bluesky

Twitter clone, all data publically available

"The firehose"

#align(center,
table(columns:3, stroke:none, column-gutter: 1em,
table.header([*Flavour*],[*Format*],[*Signatures?*]),
table.hline(),
[Original], [CBOR],[yes],
[Jetstream],[JSON],[no],
))

eg.:

#align(center)[
`wss://jetstream2.us-west.bsky.network/subscribe`
]

#speaker-note[
- Lookup someone's handle and see all their tweets
- The "ground truth" data
- Data lives on your server
- Like a blog with an RSS feed
]

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

== The firehose

#speaker-note[
- So let's take a look at it
- Here's one of their servers
- Let's see if we can get the server to talk to us
- `ws:` scheme means it wants to speak the "websockets" protocol
]

#text(size:12pt)[
#show raw: it => it.text.codepoints().join(sym.zws)

```console
$ websocat wss://jetstream2.us-west.bsky.network/subscribe
```
```json
{"did":"did:plc:l3qhzatwuymbr6ibvptiwdlw","time_us":1767409065340965,"kind":"commit","commit":{"rev":"3mbifys2ylx2m","operation":"create","collection":"app.bsky.graph.block","rkey":"3mbifys2sqh2m","record":{"$type":"app.bsky.graph.block","createdAt":"2026-01-03T02:57:46.549Z","subject":"did:plc:j74bmy3c7ls2u4mgl6xegder"},"cid":"bafyreigwmq2ihqjrk5cje2czqowbsp4t5izweswragdkjv3g7zx4qrm22m"}}
```
```json
{"did":"did:plc:g6ifxgrzi22eaemi4lc5pyr2","time_us":1767409065344776,"kind":"commit","commit":{"rev":"3mbifys4uz42r","operation":"create","collection":"app.bsky.feed.post","rkey":"3mbifys26vc2z","record":{"$type":"app.bsky.feed.post","createdAt":"2026-01-03T02:57:45.138Z","facets":[{"features":[{"$type":"app.bsky.richtext.facet#tag","tag":"Cavs"}],"index":{"byteEnd":5,"byteStart":0}}],"langs":["en"],"text":"#Cavs beat the Nuggets 113-108 to win their third straight game. Not a great performance by any means for Cleveland, but the intensity level picked up considerably down the stretch to come back from down as many as 11 in the second half."},"cid":"bafyreic7t2o4m3wdeejy46oidl7annxcpqfj243ldamylkxxo3rly3xxgi"}}
```
```json
{"did":"did:plc:r3xe3qev4vc55rsvfndqiuol","time_us":1767409065345616,"kind":"commit","commit":{"rev":"3mbifys43mq26","operation":"create","collection":"app.bsky.feed.like","rkey":"3mbifys3mxy26","record":{"$type":"app.bsky.feed.like","createdAt":"2026-01-03T02:57:45.129Z","subject":{"cid":"bafyreif7armatayafcspvi6bot4axgqzkw7q2l357k4qjxagr4vaw2rpym","uri":"at://did:plc:ecjyiwkyae3k3d46iy7d6u3h/app.bsky.feed.post/3mbid4je65s2y"}},"cid":"bafyreib3m2nlaut7ixgcwe2a6o5v3v33ixqhyy7deai2nqoehet2siapy4"}}
```
```json
{"did":"did:plc:bm3ix5bznclbq2jtst2u2q4w","time_us":1767409065347713,"kind":"commit","commit":{"rev":"3mbifyrtevq2v","operation":"delete","collection":"app.bsky.feed.post","rkey":"3mbifxsf4wc24"}}
```
```json
{"did":"did:plc:22w4xuatznlr2a65xslrqebv","time_us":1767409065348581,"kind":"commit","commit":{"rev":"3mbifyrxg2n2h","operation":"create","collection":"app.bsky.feed.like","rkey":"3mbifyrx2dn2h","record":{"$type":"app.bsky.feed.like","createdAt":"2026-01-03T02:57:44.882Z","subject":{"cid":"bafyreid6nqpuxytajk5yr2ycobx2sfoe6cucmvblhflrwdvs2qyza6jlcq","uri":"at://did:plc:dtwphuxvtvimwtra5yh4ujt4/app.bsky.feed.post/3mbiehajoke2p"}},"cid":"bafyreifmeyknmyljmd3wwjhl2wgbolkoldpngemvicram5kbtbj7oj7xni"}}
```
```json
{"did":"did:plc:73dynrh5cyibgfjskeudlwog","time_us":1767409065350834,"kind":"commit","commit":{"rev":"3mbifys5xcf2d","operation":"create","collection":"app.bsky.feed.like","rkey":"3mbifys5pif2d","record":{"$type":"app.bsky.feed.like","createdAt":"2026-01-03T02:57:46.163Z","subject":{"cid":"bafyreidaaa6ywqeibdqr7zofq3izz7obn7tb33zahbxi43j6hlo7c676ri","uri":"at://did:plc:fn5kv3xek2gi5tcjpxbiqs36/app.bsky.feed.post/3mbi5vkfugc2r"}},"cid":"bafyreiaqkz6pdolahycd4br3ppair4y7g4ysrfzai7zejcxsrzrrsffk5e"}}
```
```json
{"did":"did:plc:dwcyhpgjtqmnwzjhocewd6g2","time_us":1767409065351346,"kind":"commit","commit":{"rev":"3mbifyrwb2g2l","operation":"create","collection":"app.bsky.graph.follow","rkey":"3mbifyrvwco2l","record":{"$type":"app.bsky.graph.follow","createdAt":"2026-01-03T02:57:44.771Z","subject":"did:plc:bggek3ttucom3nj5ufe2ocu5"},"cid":"bafyreidqnbclc6dy4c7swxfj3tgip7oebnxhupdlwziwhzqywt4bgjvvry"}}
```
```json
{"did":"did:plc:jbqihdeo2viegbvjp7kq57xs","time_us":1767409065355523,"kind":"commit","commit":{"rev":"3mbifyrw3vu2q","operation":"create","collection":"app.bsky.feed.repost","rkey":"3mbifyrvp7m2q","record":{"$type":"app.bsky.feed.repost","createdAt":"2026-01-03T02:57:44.936Z","subject":{"cid":"bafyreigojgk7dpo3jkepvchqb6vu7szkdzrcn6nkobfwp4kenksqmdsywu","uri":"at://did:plc:7umb3asmcopiogfmpuxbhbbl/app.bsky.feed.post/3mbifjbmmys2s"}},"cid":"bafyreiaqzzxlkh5gimznwfuk5mfxyz47wurlwcaio7f5y74o4epr47huoq"}}

```
]

---



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

*/


== The firehose

#v(1fr)
#align(center)[
`wss://jetstream2.us-west.bsky.network/subscribe`
]
#v(2fr)

/*
// Very small, but I think you can just abount make out that it's
// JSON objects, separated by some non-ASCII stuff

// Let's zoom in on one
// Someone is _creating_ a _like_...
// So this is what gets generated every time you click the like button!

#slide[
#text(size:16pt)[
```json
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

== How much data?

\~400 events/second

Typical event: \~0.5 KiB

Bitrate: \~1.6 Mbps // (200 KiB/s)

16.5 GiB/day

#speaker-note[
Our challenge is to feed this to as many simultenous clients as possible
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
*/

== Repeaters

// Relays are nodes which crawl the PDSes and generate a firehose.

// Data is distributed to clients via a network of relays

#let tree = it => align(center, tidy-tree-graph(
  text-size: 25pt,
  node-inset: 10pt,
  spacing: (10pt, 50pt),
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
    - Repeater
      - Client
      - Client
    - Repeater
      - Client
      - Client
    - Repeater
      - Client
      - Client
],
tree[
  - Firehose
    - Repeater
      - Client
      - Client
    - #text(red)[Repeater]
      - Client
      - Client
    - Repeater
      - Client
      - Client
],
)

#speaker-note[
Some of these repeaters transform the data too, eg.
the "jetstream" format I mentioned.  Ours is just
going to re-transmit the event data verbatim;
adding transformations is easy if needed however.
]

// ---
// 1 syscall/event/client
// syscall overhead: _at least_ \~0.1μs
// => \~25k clients/core
// 0.5 KiB `write()`: a couple of μs => \~1k clients/core 😞

#speaker-note[
Ok, we're going to dive into an implementation now,
but just keep this in mind:
the name of the game is to cut down the amount of work being performed _per-client_
]
// memcpy speed: \~3 GiB/s/core  ... or is it more like 10 GiB/s ??
// => \~15k clients/core

// So... with an 8-core machine we should be able to do it!

// #speaker-note[
// Clearly the amount of data
// currently flowing through the network
// is no problem for modern hardware
// but what about the software...?
// ]

// #pause

// ...but official implementation can only do 1 Gbps 😞

// #speaker-note[
// We can do better than that

// (Note: the official server is feature-rich and the code is clearly
// designed for flexibility, not performance.  We're going to go the
// other way though: no features, high performance.)

// Let's go!
// ]

