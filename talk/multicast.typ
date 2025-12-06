#import "@preview/touying:0.6.1": *
#import "util.typ": *

= Multicast

#slide[
  // Let's take a step back and think about the problem again
  // 
  // Here's that raw data stream again

#text(size:14pt)[
#show raw: it => it.text.codepoints().join(sym.zws)
```
<81>~^A<F6>{"did":"did:plc:5rg4sbiiiesfe563zzqumetx","time_us":1763689821448290,"kind":"commit","commit":{"rev":"3m6466ot3d72m","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466ostj72m","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:21.386Z","subject":{"cid":"bafyreiatvsmobksfekpb677zhwjkzbxgpawwpluzeduuglzyq2wpyjk64a","uri":"at://did:plc:o7ozygoaxci2djvrazhdmgds/app.bsky.feed.post/3m6466aia7s2u"}},"cid":"bafyreigyuqrq7phuhl5tzyf2ek5pgeea7p4gtsrmymfb7r3a4surjmf5qi"}}<81>~^A<F6>{"did":"did:plc:htmhzec2p7aru5juw43732am","time_us":1763689821448774,"kind":"commit","commit":{"rev":"3m6466ob3d72p","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466oaomx2p","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.741Z","subject":{"cid":"bafyreidyg66ngsfvzp5xgzw6vde2okbhker54k4bndx7ilz5rrdv545rk4","uri":"at://did:plc:upogmzimjos5wbp4y5dkzt7x/app.bsky.feed.post/3m5zzb6nms22o"}},"cid":"bafyreiftxgsfr7revfoafmrbuqks3ec5pkh2zfvl6uyzdohys76yhtdxnq"}}<81>~^C7{"did":"did:plc:nutrikdii7qo6xbb52mdlyf2","time_us":1763689821449193,"kind":"commit","commit":{"rev":"3m6466nzecl2z","operation":"create","collection":"app.bsky.feed.post","rkey":"3m64666fhis2e","record":{"$type":"app.bsky.feed.post","createdAt":"2025-11-21T01:50:04.166Z","embed":{"$type":"app.bsky.embed.images","images":[{"alt":"","aspectRatio":{"height":1500,"width":2000},"image":{"$type":"blob","ref":{"$link":"bafkreifnxbxn4vlpsdawnxktr7aoeyzocooux3b2uwxy3p7avs7npubiam"},"mimeType":"image/jpeg","size":886994}},{"alt":"","aspectRatio":{"height":1500,"width":2000},"image":{"$type":"blob","ref":{"$link":"bafkreid4toh755jum2mh64ds3ehon77fom3y3qaamhr2ywnfhg4dibwkiu"},"mimeType":"image/jpeg","size":970465}}]},"langs":["ja"],"text":"乗った！"},"cid":"bafyreiecicaisnrex4akqpyxzunn2sfhl6dl6y3l42drko4ab4zhj6otxu"}}<81>~^A<8A>{"did":"did:plc:hyhxd432dqwxzrurdymoj2b5","time_us":1763689821450390,"kind":"commit","commit":{"rev":"3m6466ny6f72r","operation":"create","collection":"app.bsky.graph.follow","rkey":"3m6466nxnrx2r","record":{"$type":"app.bsky.graph.follow","createdAt":"2025-11-21T01:50:20.390Z","subject":"did:plc:iwqdab3ifz6ooighwhslpiiz"},"cid":"bafyreia3zmpvp2gig7brfcdqg7lkgoi6t4jbbxmd6ynr2rosndedpbkjme"}}<81>~^A<FA>{"did":"did:plc:bgickkrqnnv7nyfh6ylbkxwb","time_us":1763689821450842,"kind":"commit","commit":{"rev":"3m6466ooca626","operation":"create","collection":"app.bsky.feed.repost","rkey":"3m6466ontlg26","record":{"$type":"app.bsky.feed.repost","createdAt":"2025-11-21T01:50:21.111Z","subject":{"cid":"bafyreiezmx2hskvynr3ru775zy6hitmrjj27ow43ic4sx6vy53m22zjhki","uri":"at://did:plc:t5g4e7slz7mc56xlbcpobdvf/app.bsky.feed.post/3m63av4qtae2m"}},"cid":"bafyreihee3wkujchuavh3xbqesl2w67rrxjrq2h2hyohz4pmg26qtfnpbe"}}<81>~^A<F6>{"did":"did:plc:cp4mjijntnhh27pdtqi7hucg","time_us":1763689821451503,"kind":"commit","commit":{"rev":"3m6466oom4a2j","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466oo4ia2j","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.931Z","subject":{"cid":"bafyreidynbpfmymrtfwcj6uax7pmmq7y35mv4ifc6kpaifvh7fayvr2mty","uri":"at://did:plc:pbtde2womj
  ```
// 2b6sbmt2usuzyw/app.bsky.feed.post/3m63rrmcryk2e"}},"cid":"bafyreiehlccixoy34nc3mmcgxp5qndn6ullvlofmruwh75aiebwkgecwi4"}}<81>~^B<93>{"did":"did:plc:5segm472sboj3bznepzusiot","time_us":1763689821452124,"kind":"commit","commit":{"rev":"3m6466ogwke2b","operation":"create","collection":"app.bsky.feed.like","rkey":"3m6466ogiuu2b","record":{"$type":"app.bsky.feed.like","createdAt":"2025-11-21T01:50:20.918Z","subject":{"cid":"bafyreia32lc3lf6mdsdyuxx6e555sbjto2c5baegj5owg4cmmvcliicudq","uri":"at://did:plc:lrw4cnptopspeg5tthztcliw/app.bsky.feed.post/3m63g7tcnva2e"},"via":{"cid":"bafyreie
// 6ofr46v6h2zuwud5gv3zlf2pvgs5s7a3jivn6ik3sa5kjghvxd4","uri":"at://did:plc:rrfwruhud4ovela3oe6isre5/app.bsky.feed.repost/3m644ru253n25"}},"cid":"bafyreiggz52lgkwhyuo4wnf5qrdyldwqvmufunzcuvwrsxp4glchcgz4fa"}}

  // Remember I said those websocket framing bytes are basically just encoding the length?
  // In fact they do encode a few other things as well, but importantly there's nothing
  // in there that's specific to you.
  // That means that these exact bytes, this is what _everyone_ sees
  // Clients may receive them at different speeds, but in the end all clients
  // receive the same sequence of bytes
  // (after the HTTP handshake is complete)
  // This situation is called "multicast"
]]

== Multicast

// If everyone were on the same network, the technology you'd use would be
UDP multicast

// The way it works is:
// there's a block of IP addresses called the "multicast range"
// these IPs can't be assigned to individual hosts
// rather, you use them to refer to "multicast groups"
// Clients "subscribe" to a group by connecting to its IP address
// Servers "publish" to a group by sending to the IP address
// The message will be recieved by all connected clients
// Simple!
// And it's all taken care of by the network hardware itself
// so it's super efficient

Multicast over open internet: not a thing

// and anyway, the protocol we're implementing is TCP-based
TCP multicast: not a thing
// there are experiments and proposals, but none caught on

// Ok, let's start again

#slide[
  #image("sendfile.svg")
]
