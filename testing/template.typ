#show "r.v.": name => box()[random variable];
#show "p.m.f": name => box()[probability mass function];

/// old tcolor-box header
  // #align(center)[
  //   #block(fill: fill.darken(50%), inset: 4pt,  radius: (top-left: 4pt, bottom-right: 10pt))[
  //     #set text(font: "Exo 2", size: 10pt, weight: 800, fill: white)
  //     #show heading: it => block(above: 0pt, below: 0pt, inset: (left: 10pt, right: 10pt, bottom: 2pt, top: 2pt))[#set text(weight: 600, size: 10pt); #it.body];
  //     #h(5mm) #heading(level: 2)[#title] #h(5mm)
  // ]

#let problem_counter = state("__problem_counter_andy", none)

#let sol(content, fill: rgb("000000"), with: none) = {
  set align(center)
  block(width: 90%, stroke: 0.5pt + fill.lighten(40%), inset: (left: 10pt, right: 10pt, bottom: 15pt, top: 10pt), radius: 4pt)[
  #place(dx: -10pt, dy: -10pt)[
  #block(fill: fill.darken(50%), inset: 2pt,  radius: (top-left: 4pt, bottom-right: 10pt))[
    #set text(font: "Exo 2", size: 8pt, weight: 800, fill: white)
    #show heading: it => block(above: 0pt, below: 0pt, inset: (left: 10pt, right: 10pt, bottom: 2pt, top: 2pt))[#set text(weight: 600, size: 8pt); #it.body];
    #{if (with != none) {[
    #h(5mm) #heading(level: 2)[Solution w/ #with] #h(5mm)
    ]}
    else[#h(5mm) #heading(level: 2)[Solution] #h(5mm)]
  }
  ]
]
  #v(10pt)
  #set align(left)
  
  #content
]
}

#let tcolorbox(title: none, fill: rgb("f6f6f6"), content, breakable: true) = {

  // we want our real box to be offset by 10 pt
  show: block.with(inset: (top: 10pt))
  // now we want the rest of our content to be inside this box.
  show: block.with(
    width: 100%,
    fill: fill,
    inset: (left: 10pt, right: 10pt, bottom: 15pt, top: 10pt),
    radius: 4pt,
    breakable: breakable,
    stroke: 1pt + fill.darken(20%),
  )

   

  
  // now let's creat a scope to store the problem title
  {
    // now let's override the second level 
    // heading so that that the boxes showup in the 
    // pdf outline.
    show heading.where(
    level: 2
    ): it => [
      #set text(font: "Atkinson Hyperlegible", size: 12pt, weight: 500, fill: white)
      #it.body
    ]
    // we want to overhang the edge
    show: place.with(dy: -16pt)
    // and we need a width 100% block to work with the place.
    show: block.with(width: 100%)
    // now we align center within the block
    set align(center)
    // now show a block that holds the contents
    show: block.with(
      fill: fill.darken(50%),
      inset: (x: 16pt, y: 4pt),  
      radius: (top-left: 8pt, bottom-right: 8pt, top-right: 2pt, bottom-left: 2pt),
      stroke: fill.darken(40%)
    )
    // now create a heading to title the box.
    heading(level: 2, title)
  }

  
  v(6pt)
  
  content 
  
}

)
}


#let tbox(title, content, autopage: true, breakable: false, fill: rgb("f9f9f9")) = [

  #problem_counter.update(c => title)
  // add an empty strong element 
  // with a tbox label in order to create the problem label that
  // we're looking for.
  #tcolorbox(title: title, fill: fill, breakable: breakable)[#content] <tbox>
  ];

#let probcounter = counter("problem-counter");
#let Problem = [
    #{
      locate(loc => {
        let value = counter("problem-counter").at(loc).at(0)
    
        if (value != 1) { 
          pagebreak(weak: true)  
      }
    })
    }
    *Problem #probcounter.display()* #{probcounter.update(v => v + 1)} 
  ]

#let fbox(title, content) = block[
  #block(fill: black, width: 100%, stroke: 1.25pt + black, inset: 10pt, radius: (top-left: 3pt, top-right: 3pt))[
    #text(font: "Noto Sans", size: 14pt,fill: white, weight: 600)[
      #title
    ]
  ]
  #v(-5mm)
  
  #block(stroke: 1.25pt + black, width: 100%, inset: 12pt, radius: (bottom-left: 3pt, bottom-right: 3pt))[
    #content
]
]

#let proof(contents) = block(width: 100%, inset: 8pt, fill: rgb("f3f3f3"), radius: 3pt)[
  #block(width: 100%, fill: rgb("666"), outset: 8pt, radius: (top-left: 3pt, top-right: 3pt))[
    #text("Noto Sans", weight: 500, fill: white)[#align(center)[#smallcaps()[proof]]]
  ]
  #v(8pt)
  
  #contents
]


//#let thm(name, content) = [
//  #rect(width: 100%, radius: 10%, inset: 10pt)[
//  // #set text("Syne Tactile", weight: 800, 18pt)
//  #block[
//  #set text("DM Mono", style: "italic", weight: 800, 14pt)
//    #text(name)
//  ]
//
//  #text(content)
//]
//]

#let thm(name, content) = fbox(name, content)


#let conf(title: none, author: "Andy Day", professor: "Evan Randles", due: none, h2: left, class: "", doc) = {

  let due2 = if due != none { 
  [Due: #due]
  } else { 
  []
  }
  set page(
    "us-letter",
    margin: ("x": 0.75in, "y": 1in),
    header: block(width: 100%, height: 100%, inset: (top: 32pt, left: 0pt, right: 0pt, bottom: 0pt), fill: none)[
      
      #align(horizon)[
        
      #grid(columns: (0.75fr, auto, 0.75fr), row-gutter: 6pt, [Professor: #professor], align(center)[#class], 
      align(right)[ Page #counter(page).display()],
      [Student: #author],
      align(center)[
        *#title*
      ],
      align(right)[#due2],
  )
      
      
      #place(dy: 0.10in)[#line(length: 100%, stroke: 0.5pt)]]
      
      ],
      

  )
  
  v(0.5in)
  align(center)[
    #text(17pt)[#title] \
    Andy Day
  ]

  show heading.where(level: 1): it => [
    #set text(weight: 400)
    #set align(center)
    
    #smallcaps(it)
  ]
  show heading.where(
    level: 2
  ): it => [
    #set text(font: "Barlow", size: 14pt, weight: 600)
    #if h2 == center {
      align(alignment: h2)[#it.title]
    }
    #if h2 == left {
      align(left)[#it.body]
    }
  ]

  show heading.where(
    level: 3
  ): it => block[
    #set text(font: "Barlow", size: 11pt, weight: 600)
    #v(2mm)
    #text(it.title)
  ]
  show heading.where(
    level: 4
  ): it => block[
    #set text(font: "Barlow", size: 10pt, weight: 600)
    #v(2mm)
    #text(it.title)
  ]
  v(0.5in)

  let int = math.integral;

  doc
}






