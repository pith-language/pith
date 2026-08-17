---
schema: design-doc/v1
id: brand-system
title: identity system
summary: the standing visual identity — palette, type, devices, the mark's construction, the rules it answers to, and where the artifacts live
kind: design
status: accepted
created: 2026-08-19
updated: 2026-08-20
tags:
  - brand
  - identity
  - design
relations:
  informed_by:
    - foundation-name
  depends_on:
    - brand-process
  supersedes: []
---

# identity system

this is what stands. how it was chosen — the research and every round of
rejection — is in [identity process](process.md); the naming vocabulary it
shares is in [name and brand](../foundation/name.md).

## palette

| | | |
| --- | --- | --- |
| warm black | `#1A1C17` | dark ground, never pure black |
| cream | `#F0EAD8` | light ground, never pure white |
| chartreuse | `#C9D96B` | the one accent |
| pith green | `#2E5A3C` | the kernel dot; `#5E8A6E` on dark grounds |
| ink | `#23201A` | text and strokes on cream |

## type

- display — bricolage grotesque 800, opsz 96, wdth 88, lowercase wordmark
- text — gambetta 400–600
- labels — geist mono, tracked
- machine — departure mono, `-webkit-font-smoothing: none`, sizes in
  multiples of 12

all faces are free (google fonts and fontshare, ofl). the banner's wordmark
is real bricolage 800 extracted as vector paths with fonttools, because
github's svg renderer will not load webfonts; when the wordmark appears as
paths, the period is chartreuse.

## devices

- hard-shadow boxes — cream cards with a `10px 10px 0` chartreuse-tinted
  offset shadow and a 1.5px ink border.
- roman numerals — section numbers set small in mono.
- flat color only — no gradients, no glow, no glass.

the hollow numeral from the monument study — a measurement drawn outline-only
in chartreuse — is not a standing device. it earned its moment inside that
study, but a number floating in the banner or the hero needs an explanation
the viewer does not have, which makes it decoration. it was rejected there
twice: first the golden-angle content, then the rotation angle swapped in to
replace it. numbers appear where a reader needs them, as in the mark
specification below, and are set plainly.

## the mark

unit — one marquise petal, tip radii 41 and 11 on a 100-unit viewbox, curve
controls at exact thirds of the span so the widest point sits on the petal's
midpoint. rule — five at 72°. accent — one petal chartreuse, at 216°.
kernel — pith green dot, radius 4.2, set in the void. the void is a
five-pointed star by construction. below 20px the dot folds away.

five petals read as the five sealed effect categories around one kernel —
the mark says the architecture. any crop of the construction still names
it, which was the nix test.

## rules

- no generated-look patterns — custom palettes, asymmetric layouts, a
  component vocabulary of one's own
- not a tech-tool aesthetic — no glow, no glass, no gradient depth
- no italic, ever — brand pages carry `em, i { font-style: normal }` as a
  physical enforcement
- no plain sans as the identity face
- fully free fonts only
- no cheap-feeling free faces, no newspaper faces
- no pure white or pure black — always offwhite or offblack
- departure mono for machine labels only
- the mark is a construction, not a decorated shape — a repeated unit under
  a rotation rule
- fragments are a test, not a component

## artifacts

- [`docs/assets/banner.svg`](../assets/banner.svg) — the readme banner,
  960×240, flipping cream and warm black through `prefers-color-scheme` for
  github's light and dark modes.
- [`docs/assets/favicon.svg`](../assets/favicon.svg) — the 16px mark:
  enlarged petals on a warm-black tile, no dot.
- [`docs/assets/mark.svg`](../assets/mark.svg),
  [`docs/assets/mark-dark.svg`](../assets/mark-dark.svg) — the canonical
  mark in both modes, transparent.
- [`docs/assets/ledger.svg`](../assets/ledger.svg) — the readme's
  incremental-build figure, in the palette: cream and warm black flipping
  with github's scheme, mono labels and numerals, pith green marking the
  zero-work outcomes.
