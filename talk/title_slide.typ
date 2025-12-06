#set page(fill: maroon)
#set text(fill: white)
#set align(center)

#show heading: underline.with(
  background: true,
  stroke: (thickness: .3em, paint: purple.darken(50%), cap: "round"),
  evade: false,
  extent: .2em,
)
#show heading: set text(size: 40pt)

#v(1fr)
= #h(-6em) Let's write!
#v(-1em)
= #h(2em) A relay for Bluesky

#v(1fr)

#text(style:"italic", size: 25pt )[
High-throughput networking \
with Rust and Linux
]

#v(1fr)
Alex Sayers
#v(1fr)

// #friendly.title-slide(
//   title: [Let's write a Bluesky relay!],
//   speaker: [Alex Sayers],
//   conference: [An example of how to write high-throughput networking software with Rust and Linux],
//   speaker-website: none, // "url-to-the-speaker.org", // use `none` to disable
//   slides-url: "URL to slides", // use `none` to disable
//   qr-caption: text(font: "Excalifont")[Get these slides],
//   logo: none, // auto,
// )
