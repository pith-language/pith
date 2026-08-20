import { mkdir, writeFile } from 'node:fs/promises';
import { loadOgFonts, ogCard, renderOgCard } from '../src/og/layout.mjs';

const cards = {
  default: ogCard({
    line: 'a computation kernel for build, package, environment, and system tooling.',
  }),
  home: ogCard({
    line: 'the mechanism the tools could have shared.',
  }),
};

const fonts = await loadOgFonts();
const outDir = new URL('../public/og/', import.meta.url);
await mkdir(outDir, { recursive: true });

for (const [name, card] of Object.entries(cards)) {
  const png = await renderOgCard(card, { fonts });
  await writeFile(new URL(`${name}.png`, outDir), png);
  console.log(`og/${name}.png  ${png.byteLength} bytes`);
}
