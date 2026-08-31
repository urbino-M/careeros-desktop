#let data = json("cv-data.json")
#let burgundy = rgb("#ad003d")
#let section-rule = rgb("#b7d67a")
#let quiet = rgb("#3f4d50")

#set page(
  paper: "a4",
  margin: (x: 10.5mm, y: 9mm),
  numbering: none,
)
#set text(
  font: ("Times New Roman", "New Computer Modern", "Libertinus Serif"),
  size: 9.45pt,
  fill: rgb("#15191a"),
  lang: "en",
)
#set par(justify: true, leading: 0.43em)

#let section-title(title) = block(
  above: 7.2pt,
  below: 3.6pt,
  breakable: false,
)[
  #text(size: 13.8pt, weight: "bold")[#title]
  #v(-1.8pt)
  #line(length: 100%, stroke: 1.25pt + section-rule)
]

#let styled-body(section, body) = {
  if section == "Research Profile" {
    let parts = body.split(":")
    if parts.len() > 1 {
      strong(parts.at(0) + ":")
      h(2pt)
      parts.slice(1).join(":")
    } else { body }
  } else if section == "Education" or section.contains("Research Experience") {
    let parts = body.split(". ")
    if parts.len() > 1 {
      strong(parts.at(0) + ".")
      h(2pt)
      parts.slice(1).join(". ")
    } else { body }
  } else { body }
}

#let entry(section, item) = grid(
  columns: (4.5mm, 1fr, 31mm),
  column-gutter: 2mm,
  row-gutter: 0pt,
  inset: (y: 1.35pt),
  [#align(top)[#v(1.4pt)#rect(width: 3.4pt, height: 8pt, fill: burgundy)]],
  [#styled-body(section, item.body)],
  [#align(right)[#text(size: 8.8pt, fill: quiet)[#item.key]]],
)

#align(left)[
  #text(size: 18.5pt, weight: "bold")[#data.name]
  #v(0.8pt)
  #text(size: 9.7pt)[#data.tagline]
  #v(1.8pt)
  #grid(
    columns: (4mm, 1fr),
    column-gutter: 1.5mm,
    [#text(size: 9pt, weight: "bold", fill: burgundy)[✉]],
    [#text(size: 8.9pt)[#data.contact]],
  )
  #for affiliation in data.affiliations.split(" · ") [
    #grid(
      columns: (4mm, 1fr),
      column-gutter: 1.5mm,
      [#text(size: 8.5pt, weight: "bold", fill: burgundy)[■]],
      [#text(size: 8.9pt)[#affiliation]],
    )
  ]
]

#for section in data.sections [
  #if section.title.contains("(continued)") [#pagebreak()]
  #section-title(section.title)
  #for item in section.entries [#entry(section.title, item)]
]
