#import "@preview/touying:0.6.1": *
#import "util.typ": *

= Websockets

== Websockets

#speaker-note[
Can we go faster?

If we want to optimise something, the first step is to understand it.  So let's dig in.

The WebSocket protocol is an IETF standard, which means you get a nice RFC to read.

In this case it's mercifully very short and readable.  Here's how it works.
]

// RFC 6455

#grid(columns:(1fr,14cm,1fr), [],
text(13pt)[
```text
Internet Engineering Task Force (IETF)                          I. Fette
Request for Comments: 6455                                  Google, Inc.
Category: Standards Track                                    A. Melnikov
ISSN: 2070-1721                                               Isode Ltd.
                                                           December 2011


                         The WebSocket Protocol

Abstract

   The WebSocket Protocol enables two-way communication between a client
   running untrusted code in a controlled environment to a remote host
   that has opted-in to communications from that code.  The security
   model used for this is the origin-based security model commonly used
   by web browsers.  The protocol consists of an opening handshake
   followed by basic message framing, layered over TCP.  The goal of
   this technology is to provide a mechanism for browser-based
   applications that need two-way communication with servers that does
   not rely on opening multiple HTTP connections (e.g., using
   XMLHttpRequest or <iframe>s and long polling).
```
], [])

---

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

/*
#slide[

// ```
// $ head -c 16 /dev/urandom | base64
// ZrA4jlaiSs09uWIE5yW2Tg==
// ```

// ```
// $ cat headers
// GET /subscribe HTTP/1.1
// Host: jetstream2.us-west.bsky.network
// Upgrade: websocket
// Connection: Upgrade
// Sec-WebSocket-Key: ZrA4jlaiSs09uWIE5yW2Tg==
// Sec-WebSocket-Version: 13
//
// ```

// (Empty line terminates the header list)

// $ cat headers | sed 's/$/\r/' | ncat --ssl jetstream2.us-west.bsky.network 443

#show raw: it => it.text.codepoints().join(sym.zws)

#set text(size: 14pt)
```
$ printf "GET /subscribe HTTP/1.1\r\n..." | ncat --ssl jetstream2.us-west.bsky.network 443
HTTP/1.1 101 Switching Protocols
upgrade: websocket
connection: Upgrade
sec-websocket-accept: r9GITgmWV51X48CepKZ0m7U5uNs=

�~�{"did":"did:plc:mrjnzopplbl5r7spqmuyzh7v","time_us":1767416549047266,"kind":"commit","commit":{"rev":"3mbimxszk3p2f","operation":"create","collection":"app.bsky.feed.like","rkey":"3mbimxsz5fh2f","record":{"$type":"app.bsky.feed.like","createdAt":"2026-01-03T05:02:28.717Z","subject":{"cid":"bafyreiftrbph3hv6mlwa4l5rhmt7wk6yvutclpocjyut63jh34l2vz3vr4","uri":"at://did:plc:krvfulsq6n4frvt32kuoo4cj/app.bsky.feed.post/3mbidgoynac2s"}},"cid":"bafyreicalgqo4eojzk5j5t7dbdq2ey5isegw6nvp36ohxyldqyoqngv26e"}}�~�{"did":"did:plc:uo4sbqoxkhwjgby6z6mflmgz","time_us":1767416549048052,"kind":"commit","commit":{"rev":"3mbimxt55f22z","operation":"create","collection":"app.bsky.feed.repost","rkey":"3mbimxt4ro22z","record":{"$type":"app.bsky.feed.repost","createdAt":"2026-01-03T05:02:29.551Z","subject":{"cid":"bafyreidurkqqh2th5n6dc3hrjiaolx4vfumbxw6i656zc3v7ip7ru3zbfq","uri":"at://did:plc:qocfyf7cua337thual5h7kss/app.bsky.feed.post/3mb5xhh6ta22a"},"via":{"cid":"bafyreic7f5pgarxoste4qt3pkwszw77gohlrrjsu5umombjo7kd6ek6dxa","uri":"at://did:plc:xwsaekkdguwxzeks5d2jxzyp/app.bsky.feed.repost/3mb6ajhaprv2a"}},"cid":"bafyreigam47mm75lkfu6muvismboy7wvnfpf4psrhptzpj3xwr2ws3s2mi"}}�~�{"did":"did:plc:4acp7jb7ryban2xfeyzfynq7","time_us":1767416549049045,"kind":"commit","commit":{"rev":"3mbimxt27is2t","operation":"create","collection":"app.bsky.feed.repost","rkey":"3mbimxszqu22t","record":{"$type":"app.bsky.feed.repost","createdAt":"2026-01-03T05:02:28.455Z","subject":{"cid":"bafyreifj5yqa4ibldkk4og3hqhfaz7obpapg5lq7gmkttxkayrrxnhouey","uri":"at://did:plc:kzxl37blybhp7kvn2clme7j2/app.bsky.feed.post/3mbhc3udntk2v"},"via":{"cid":"bafyreicbx7h5xlrl5fe4pd35jkrvb2idbkg4kihtslo3hu26elpqysevsu","uri":"at://did:plc:74jisjswlqeycuecdjrp25l2/app.bsky.feed.repost/3mbimcdjlf322"}},"cid":"bafyreihn62cd4mjx7pvwwtjzcgzibrgu3oohugzjguxzywbfj52y5mdche"}}�~�{"did":"did:plc:xswtrw4u7bdzj4viyz3cc553","time_us":1767416549050234,"kind":"commit","commit":{"rev":"3mbimxt3hsh26","operation":"create","collection":"app.bsky.feed.like","rkey":"3mbimxt2x7726","record":{"$type":"app.bsky.feed.like","createdAt":"2026-01-03T05:02:28.763Z","subject":{"cid":"bafyreigozz6dndqoa4e5sdhi2pumj26kch7eefzvcmvqjx2zstogp4v56u","uri":"at://did:plc:pa5k3okjdjufnxdivp6hhnv6/app.bsky.feed.post/3mbhyp7szek2d"},"via":{"cid":"bafyreibqhaexjjpwd5a4lc5ohur7fi7xy6tihj2p7cfo5kccs6mrb5jt7m","uri":"at://did:plc:5tbowzedh5d6wvhc5dncydbx/app.bsky.feed.repost/3mbimughafk2b"}},"cid":"bafyreievdznksfkker5tmunxza2qaahr3jzouwytirufxfwrm6cuu2uefy"}}�~�{"did":"did:plc:gf5p6zgrsbawlb3mqgsmwsk5","time_us":1767416549051428,"kind":"commit","commit":{"rev":"3mbimxt6ylt2v","operation":"create","collection":"app.bsky.feed.like","rkey":"3mbimxt6ixt2v","record":{"$type":"app.bsky.feed.like","createdAt":"2026-01-03T05:02:28.726Z","subject":{"cid":"bafyreigvc2kpdly46w7sqztpewiteyq4whcflm2xu5u23s3krxwivg236y","uri":"at://did:plc:2dpglnovtraniqvyh57jo6tm/app.bsky.feed.post/3mbi5twfwdc2b"}},"cid"
```

#speaker-note[
Let's take a step back.
Here's that raw data stream again.

Websockets presents you with a series of "frames";
but at the TCP level it's just a continuous stream of bytes.

The websocket frames are delimited by those headers there;
but TCP doesn't see any difference between those bytes and the JSON around them

When we call write(), it doesn't have to line up with the frames.
We can send multiple frames, or part of a frame, or whatever.

Furthermore, we know that the same JSON is being sent to all clients;
but actually the websocket headers are the same too
There's nothing client-specific about them.

That means that _this exact stream of bytes_ --- this is what every client will recieve... eventually.
Different clients may be at different points in the stream ---
because some clients are fast and others are slow ---
but eventually everyone sees the same bytes as everyone else.

(... after the HTTP handshake, that is.)
]
]

// #slide[
// #text(size:14pt)[
// #show raw: it => it.text.codepoints().join(sym.zws)
// ```
// <81>~^A<F6>{"did":"did:plc:5rg4sbiiiesfe563zzqumetx","time_us":1763689821448290,"kind":"commit","commit":{"rev":"3m6466ot3d72m","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466ostj72m","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:21.386Z","subject":{"cid":"bafyreiatvsmobksfekpb677zhwjkzbxgpawwpluzeduuglzyq2wpyjk64a","uri":"at://did:plc:o7ozygoaxci2djvrazhdmgds/app.bsky.feed.post/3m6466aia7s2u"}},"cid":"bafyreigyuqrq7phuhl5tzyf2ek5pgeea7p4gtsrmymfb7r3a4surjmf5qi"}}<81>~^A<F6>{"did":"did:plc:htmhzec2p7aru5juw43732am","time_us":1763689821448774,"kind":"commit","commit":{"rev":"3m6466ob3d72p","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466oaomx2p","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.741Z","subject":{"cid":"bafyreidyg66ngsfvzp5xgzw6vde2okbhker54k4bndx7ilz5rrdv545rk4","uri":"at://did:plc:upogmzimjos5wbp4y5dkzt7x/app.bsky.feed.post/3m5zzb6nms22o"}},"cid":"bafyreiftxgsfr7revfoafmrbuqks3ec5pkh2zfvl6uyzdohys76yhtdxnq"}}<81>~^C7{"did":"did:plc:nutrikdii7qo6xbb52mdlyf2","time_us":1763689821449193,"kind":"commit","commit":{"rev":"3m6466nzecl2z","operation":"create","collection":"app.bsky.feed.post","rkey":"3m64666fhis2e","record":{"$type":"app.bsky.feed.post","createdAt":"2025-11-21T01:50:04.166Z","embed":{"$type":"app.bsky.embed.images","images":[{"alt":"","aspectRatio":{"height":1500,"width":2000},"image":{"$type":"blob","ref":{"$link":"bafkreifnxbxn4vlpsdawnxktr7aoeyzocooux3b2uwxy3p7avs7npubiam"},"mimeType":"image/jpeg","size":886994}},{"alt":"","aspectRatio":{"height":1500,"width":2000},"image":{"$type":"blob","ref":{"$link":"bafkreid4toh755jum2mh64ds3ehon77fom3y3qaamhr2ywnfhg4dibwkiu"},"mimeType":"image/jpeg","size":970465}}]},"langs":["ja"],"text":"乗った！"},"cid":"bafyreiecicaisnrex4akqpyxzunn2sfhl6dl6y3l42drko4ab4zhj6otxu"}}<81>~^A<8A>{"did":"did:plc:hyhxd432dqwxzrurdymoj2b5","time_us":1763689821450390,"kind":"commit","commit":{"rev":"3m6466ny6f72r","operation":"create","collection":"app.bsky.graph.follow","rkey":"3m6466nxnrx2r","record":{"$type":"app.bsky.graph.follow","createdAt":"2025-11-21T01:50:20.390Z","subject":"did:plc:iwqdab3ifz6ooighwhslpiiz"},"cid":"bafyreia3zmpvp2gig7brfcdqg7lkgoi6t4jbbxmd6ynr2rosndedpbkjme"}}<81>~^A<FA>{"did":"did:plc:bgickkrqnnv7nyfh6ylbkxwb","time_us":1763689821450842,"kind":"commit","commit":{"rev":"3m6466ooca626","operation":"create","collection":"app.bsky.feed.repost","rkey":"3m6466ontlg26","record":{"$type":"app.bsky.feed.repost","createdAt":"2025-11-21T01:50:21.111Z","subject":{"cid":"bafyreiezmx2hskvynr3ru775zy6hitmrjj27ow43ic4sx6vy53m22zjhki","uri":"at://did:plc:t5g4e7slz7mc56xlbcpobdvf/app.bsky.feed.post/3m63av4qtae2m"}},"cid":"bafyreihee3wkujchuavh3xbqesl2w67rrxjrq2h2hyohz4pmg26qtfnpbe"}}<81>~^A<F6>{"did":"did:plc:cp4mjijntnhh27pdtqi7hucg","time_us":1763689821451503,"kind":"commit","commit":{"rev":"3m6466oom4a2j","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466oo4ia2j","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.931Z","subject":{"cid":"bafyreidynbpfmymrtfwcj6uax7pmmq7y35mv4ifc6kpaifvh7fayvr2mty","uri":"at://did:plc:pbtde2womj
//   ```
// // 2b6sbmt2usuzyw/app.bsky.feed.post/3m63rrmcryk2e"}},"cid":"bafyreiehlccixoy34nc3mmcgxp5qndn6ullvlofmruwh75aiebwkgecwi4"}}<81>~^B<93>{"did":"did:plc:5segm472sboj3bznepzusiot","time_us":1763689821452124,"kind":"commit","commit":{"rev":"3m6466ogwke2b","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466ogiuu2b","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.918Z","subject":{"cid":"bafyreia32lc3lf6mdsdyuxx6e555sbjto2c5baegj5owg4cmmvcliicudq","uri":"at://did:plc:lrw4cnptopspeg5tthztcliw/app.bsky.feed.post/3m63g7tcnva2e"},"via":{"cid":"bafyreie
// // 6ofr46v6h2zuwud5gv3zlf2pvgs5s7a3jivn6ik3sa5kjghvxd4","uri":"at://did:plc:rrfwruhud4ovela3oe6isre5/app.bsky.feed.repost/3m644ru253n25"}},"cid":"bafyreiggz52lgkwhyuo4wnf5qrdyldwqvmufunzcuvwrsxp4glchcgz4fa"}}

// ]]

// == Multicast

// // Ok, let's start again

// #slide[
//   #image("sendfile.svg")
// ]

*/
