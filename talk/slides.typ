// #let accent1 = rgb("637A9F")
// #let accent3 = rgb("80B9AD")
// #let accent4 = rgb("B3E2A7")
#set page(
  paper: "presentation-16-9",
  // margin: 1em,
)

#set text(size: 30pt, font: "Andika")
#show raw: set text(font: "Iosevka")
// #show math.equation: set text(font: "Lete Sans Math")

#include("title_slide.typ")

#import "@preview/touying:0.6.1": *
#import themes.simple: *
#show: simple-theme.with(
  header: none,
  header-right: none,
  footer: self => utils.display-current-heading(
    setting: utils.fit-to-width.with(grow: false, 100%),
    level: 1,
    depth: self.slide-level,
  ),
  footer-right: context utils.slide-counter.display(),
  config-common(new-section-slide-fn: none),
)
#let accent2 = rgb("E8C872")
#show heading.where(level: 2): underline.with(
  background: true,
  stroke: (thickness: .3em, paint: accent2.lighten(50%), cap: "round"),
  evade: false,
  extent: .2em,
)
#show heading: set block(below: 1em)

#include("1-bluesky.typ")
#include("2-tokio.typ")
#include("3-websockets.typ")
#include("4-adaptive.typ")
#include("5-zero-copy.typ")
// #include("impl-3.typ")
#include("6-io_uring.typ")
// #include("io-uring.typ")
// #include("impl-4.typ")
// #include("hole-punch.typ")
#include("conclusion.typ")
#include("multicast.typ")

// #friendly.last-slide(
//   title: [That's it!],
//   project-url: "URL to project",
//   qr-caption: text(font: "Excalifont")[My project on GitHub],
//   contact-appeal: [Get in touch #emoji.hand.wave],
//   // leave out any of the following if they don't apply to you:
//   email: "foo@bar.org",
//   mastodon: "@foo@baz.org",
//   website: "bar.org"
// )
