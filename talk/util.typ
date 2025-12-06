#let accent1 = rgb("637A9F")
#let titled-block(title: [], body, ..kwargs) = {
  stack(
    dir: ttb,
    spacing: 5pt,
    text(
      size: .8em,
      fill: accent1,
      sym.triangle.small.stroked.r + sym.space + title
    ),
    block(
      inset: 10pt,
      width: 100%,
      stroke: 2pt + accent1.lighten(50%),
      ..kwargs.named(),
      body
    )
  )
}

#let small = it => text(size:20pt, style:"italic", fill:gray, it);
