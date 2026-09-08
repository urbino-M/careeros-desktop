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
  size: 10pt,
  fill: rgb("#15191a"),
  lang: "en",
)
#set par(justify: true, leading: 0.36em)

#let section-title(title) = block(
  above: 5.4pt,
  below: 2.6pt,
  breakable: false,
)[
  #text(size: 13.8pt, weight: "bold")[#title]
  #v(-9pt)
  #line(length: 100%, stroke: 1.25pt + section-rule)
]

#let author-body(body) = {
  if data.authorName == "" {
    body
  } else {
    let parts = body.split(data.authorName)
    for (index, part) in parts.enumerate() {
      part
      if index < parts.len() - 1 { strong(data.authorName) }
    }
  }
}

#let styled-body(section, body) = {
  let title = lower(section)
  if title.contains("research outputs") or title.contains("publication") or title.contains("papers") or title.contains("articles") or title.contains("论文") or title.contains("出版") {
    author-body(body)
  } else if title == "research profile" or title == "target alignment" {
    let parts = body.split(":")
    if parts.len() > 1 {
      strong(parts.at(0) + ":")
      h(2pt)
      parts.slice(1).join(":")
    } else { body }
  } else if title.contains("education") or title.contains("academic background") or title.contains("project") or title.contains("research experience") or title.contains("教育") or title.contains("学历") {
    let parts = body.split(". ")
    if parts.len() > 1 {
      strong(parts.at(0) + ".")
      h(2pt)
      parts.slice(1).join(". ")
    } else { body }
  } else { body }
}

#let entry(section, item) = grid(
  columns: (2.8mm, 1fr, 34.7mm),
  column-gutter: 1mm,
  row-gutter: 0pt,
  inset: (y: 0.9pt),
  [#align(top)[#v(1.4pt)#rect(width: 3.4pt, height: 8pt, fill: burgundy)]],
  [
    #context {
      let position = here().position()
      [#metadata((kind: "entry", page: counter(page).get().first(), y: position.y.pt(), characters: item.key.len() + item.body.len())) <cv-entry>]
    }
    #styled-body(section, item.body)
  ],
  [#align(right)[#text(size: 8.8pt, fill: quiet)[#item.key]]],
)

#align(left)[
  #text(size: 18.5pt, weight: "bold")[#data.name]
  #v(-11pt)
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

#for (index, section) in data.sections.enumerate() [
  #section-title(section.title)
  #for item in section.entries [#entry(section.title, item)]
]

#context {
  let position = here().position()
  [#metadata((kind: "contentEnd", page: counter(page).get().first(), y: position.y.pt())) <cv-content-end>]
}
