---
schema: design-doc/v1
id: brand-process
title: identity process
summary: the working session that produced the visual identity — the research, the rounds of rejection, and the reason each register was discarded
kind: design
status: accepted
created: 2026-08-19
updated: 2026-08-20
tags:
  - brand
  - identity
  - history
relations:
  informed_by:
    - foundation-name
  depends_on: []
  supersedes: []
---

# identity process

the visual identity came out of a working session on 2026-08-19 and
2026-08-20. directions were built as html, judged rendered in a browser, and
narrowed one register at a time until a single system remained. this records
the sequence and the reason each round was discarded. what stands today is in
[identity system](system.md).

## the brief

the starting constraints:

- several directions, not one
- webfonts served from public cdns, free licenses only
- the existing readme diagrams are not design references; start from zero
- departure mono is interesting for machine labels, not as the primary face
- typography carries the identity

## what was studied

dev tools and landing pages: evil martians' review of 100 dev-tool landing
pages, warp's warm-times-precise principle, teenage engineering. the current
polish register: frontend horse's "the linear look", vercel's geist, "behind
the gradient" on stripe. avoiding the generated look: the ai-look fix guide
(no default purple, no rounded-card-plus-shadow, asymmetric layouts, custom
palettes). classic systems: the nasa 1975 graphics standards manual, otl
aicher's munich '72 grid, the swiss/international typographic style, the
nixos branding repo, the rust brand guide. general craft: nn/g visual
principles and brutalism-antidesign pieces, cxl on persuasion, material
dark-theme contrast guidance, awwwards sites of the year, foundries (klim,
grilli type, dinamo), fontshare, crt terminal craft values. two practical
tools came out of it: wcag contrast math applied to every proposed text
color, and the 60-30-10 proportion rule.

## the rounds

### round one — four directions, discarded as generic

field herbarium (botanical paper), phosphor kernel (green-on-black
terminal), drafting room (white, light-blue grid, ibm plex), ledger brutal
(black, bone, international orange, space grotesk). a contrast audit found
four failing text colors and they were recomputed. the round was discarded
on two grounds: it read as basic, and several of its layout patterns were
generated-look tells. a type-led rebuild — a specimen book with an
italic-serif herbarium press spread, a departure mono terminal spread, a
silver standards-manual spread, a wide-open poster spread — met the same
verdict.

### round two — the gold-standard register, discarded as not pith

aurora kernel (general sans, glass bento, aurora spills), verdigris
(fraunces 300 italic over ink-green and gold), daylight (warm white, pastel
mesh, frosted cards): the linear/vercel/stripe register, executed with
custom green/gold palettes rather than defaults. discarded whole: pith does
not dress like a tech tool. glow, glass, and gradient depth are out.

### round three — serifs, discarded twice

first the antique print faces — cormorant garamond 300, bodoni moda 800, im
fell english, eb garamond, a handwriting face for margin notes — across an
engraved botanical plate, an apothecary label, a two-ink riso poster, and
fashion folios. they read as imitation luxury or novelty. then the premium
free faces — zodiak, gambetta, boska, marcellus, fraunces — which fixed the
license and the cheapness but kept the wrong register. two standing rules
came out of this round: fully free fonts only, and never plain white or
plain black — always offwhite or offblack.

the decisive rejection: the round kept leaning on italic serif, the slanted
calligraphic style behind lines like "a plant, not a machine." and "medulla
— the central tissue". italic is banned outright. every letter stands
upright.

### round four — three upright directions, all three kept

- **monument** — museum-wall type. gloock, a contemporary didone, at up to
  280px; one enormous word on the wall, a small mono label beside it.
- **soft brutal** — flat poster. bricolage grotesque 800, heavy and soft;
  cream, chartreuse, warm off-black; no glow, no gradients, flat ink only;
  departure mono for machine labels.
- **literary** — book covers. instrument serif, hard-shadow boxes, roman
  numerals, the mark as a blind stamp.

all three landed. the ranking put soft brutal first on type and color,
monument second on spacing and grounds, literary third on boxes and numerals
with its serif running too tall. the logo was the one thing none of them got
right.

## the merge

the final draft folded the ranking together: soft brutal as the base —
bricolage 800, cream on warm black, chartreuse, flat ink — monument's calm
spacing, literary's hard-shadow boxes and roman numerals set in mono. the
tall serif was retired. the bracketed labels were dropped.

## the mark lab

the first lab offered five marks in the merged palette: cell (hexagon),
spokes (root node, five dependents), rings (stem cross-section), stem (bar,
leaf, node), seed (pointed oval). all five were rejected with a diagnosis
that named the missing principle: the nix mark is not a shape, it is a
**construction** — one unit repeated under a rotation rule, so a fragment of
it is still unmistakable. these five were decorated primitives.

the second lab followed the recipe: a repeated unit, a rotation rule,
exactly one chartreuse element, and a fragment test on every card. four
constructions: vessels (paired bars at 90°), sectors (thick arcs at 120°),
star (pointed petals at 72°), cell-key (hexagon with an exiting vessel
pair). sectors and star survived, each refined three ways with fragments as
honest crops of the full mark's viewbox and every variant in a lockup with
the wordmark. the runoff came down to three — sectors cut, star marquise,
star kernel.

fragments were then retired entirely. they were the proof of the nix
property — that a crop still names the mark — never a component of the
brand.

## the choice

c1, the marquise star: five marquise petals at 72°, one chartreuse, the
green kernel dot set in the five-pointed void the petals carve. the final
pass tightened the petal so its curve controls sit at exact thirds of its
span — the widest point lands on the petal's midpoint and the void comes out
even.
