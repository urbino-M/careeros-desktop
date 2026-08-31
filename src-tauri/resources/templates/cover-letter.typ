#let data = json("cover-letter-data.json")
#let navy = rgb("#173f54")
#let rulegray = rgb("#a7b1be")
#let quiet = rgb("#647176")

#set page(
  paper: "a4",
  margin: (left: 21.5mm, right: 21.5mm, top: 17.5mm, bottom: 16.5mm),
  numbering: none,
  footer: align(center)[
    #text(size: 7.6pt, fill: quiet)[#data.footer]
  ],
)
#set text(
  font: ("Times New Roman", "New Computer Modern", "Libertinus Serif"),
  size: 9.7pt,
  fill: rgb("#15191a"),
  lang: "en",
)
#set par(justify: true, leading: 0.5em)

#text(size: 18.5pt, weight: "bold", fill: navy)[#data.name]
#v(1.5pt)
#for line in data.headlineLines [
  #text(size: 8.65pt)[#line]
  #linebreak()
]
#text(size: 8.6pt)[#data.contact]
#v(4pt)
#line(length: 100%, stroke: 0.8pt + rulegray)
#v(6pt)

#data.date
#v(6pt)

#for line in data.recipientLines [
  #line
  #linebreak()
]
#v(5pt)

#strong[Re: #data.subject]
#v(7pt)

#data.greeting
#v(4.8pt)

#for paragraph in data.paragraphs [
  #paragraph
  #v(4.8pt)
]

#data.closing
#v(5pt)
#strong[#data.signature]
