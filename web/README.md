# pith web

The pith home page, built with Astro + Svelte. The design notebook lives in
the repository's `docs/` tree and is not part of the site yet. Every visual
choice follows [`docs/brand/system.md`](../docs/brand/system.md).

## Working here

Node and pnpm come from the repository's nix devshell. From the repository
root, the same commands are wrapped as `just web-dev`, `just web-build`, and
`just web-check`.

```sh
nix develop ..    # from web/
pnpm install
pnpm dev
```

| script | action |
| :-- | :-- |
| `pnpm dev` | dev server at `localhost:4321` |
| `pnpm og` | regenerate the OG cards into `public/og/` |
| `pnpm build` | regenerate the OG cards, then production build into `dist/` |
| `pnpm preview` | serve the production build |
| `pnpm check` | `astro check` diagnostics |

## OG cards

Every page's social card renders at build time from one shared frame.
[`src/og/layout.mjs`](src/og/layout.mjs) owns the identity: the palette from
`docs/brand/system.md`, the self-hosted faces, the mark's exact petal
construction as SVG, and the `ogCard()` layout: the wordmark and one summary
line against the mark, sized to stay legible in small previews. A page
supplies only that line. To add a card, compose one in
[`scripts/generate-og.mjs`](scripts/generate-og.mjs) and pass its path under
`public/og/` to the `ogImage` prop of `Base.astro`; pages without one fall
back to `og/default.png`. Rendering goes through [takumi](https://takumi.kane.tw/docs/)
(no headless browser), and output is byte-stable, so the images are committed.

## Deployment

The site is static (`astro build` into `dist/`) and serves from Cloudflare
Workers static assets at **pith-lang.org**. `web/wrangler.jsonc` points the
`pith-web` worker at `dist/` and attaches `pith-lang.org` as a custom
domain, which Cloudflare resolves to a DNS record and certificate on its own.

```sh
wrangler login          # once
just web-deploy         # builds, then wrangler deploy
```

Deploys can also run from CI with `CLOUDFLARE_API_TOKEN` set. The
compatibility date is 2026-05-25, the newest the nixpkgs wrangler can run
locally through `wrangler dev`; the site is static, so the date changes
nothing in production. Bump it when the devshell's wrangler updates. An early
`pith-web` Pages project still exists from the first deploy attempt and can
be deleted in the dashboard.

## Fonts

All faces are self-hosted under the SIL Open Font License 1.1. Geist Mono
arrives through `@fontsource/geist-mono`; the rest are woff2 files in
`src/fonts/`.

| face | role | source |
| :-- | :-- | :-- |
| Bricolage Grotesque | display (800, opsz 96, wdth 88) | [Google Fonts](https://fonts.google.com/specimen/Bricolage+Grotesque) |
| Gambetta | text (400–600) | [Fontshare](https://www.fontshare.com/fonts/gambetta) |
| Geist Mono | labels | [Geist by Vercel](https://github.com/vercel/geist-font), via fontsource |
| Departure Mono | machine labels only | [departuremono.com](https://departuremono.com/), © Helena Zhang |


## funding.json

The `funding.json` is emmited by the internal `pith/funding` repository, and should not be edited manually
