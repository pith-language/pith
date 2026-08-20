import { readFile } from 'node:fs/promises';
import { render } from 'takumi-js';
import { fromHtml } from 'takumi-js/helpers/html';

export const palette = {
  ground: '#F0EAD8',
  ink: '#23201A',
  sub: '#4E4936',
  line: '#938E82',
  accent: '#C9D96B',
  pith: '#2E5A3C',
};

export const CARD_WIDTH = 1200;
export const CARD_HEIGHT = 630;

const PETAL = 'M50,9 C56,19 56,29 50,39 C44,29 44,19 50,9 Z';

export function markSvg({
  size = 300,
  petals = palette.ink,
  accent = palette.accent,
  kernel = palette.pith,
} = {}) {
  const petal = (deg) => {
    const rotate = deg ? ` transform="rotate(${deg} 50 50)"` : '';
    const fill = deg === 216 ? ` fill="${accent}"` : '';
    return `<path d="${PETAL}"${rotate}${fill}/>`;
  };
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"` +
    ` width="${size}" height="${size}">` +
    `<g fill="${petals}">${[0, 72, 144, 216, 288].map(petal).join('')}</g>` +
    `<circle cx="50" cy="50" r="4.2" fill="${kernel}"/>` +
    `</svg>`;
  return new TextEncoder().encode(svg);
}

export async function loadOgFonts() {
  const [bricolage, gambetta, geist400, geist500] = await Promise.all([
    readFile(new URL('../fonts/BricolageGrotesque-VF-latin.woff2', import.meta.url)),
    readFile(new URL('../fonts/Gambetta-400.woff2', import.meta.url)),
    readFile(
      new URL(
        '../../node_modules/@fontsource/geist-mono/files/geist-mono-latin-400-normal.woff2',
        import.meta.url,
      ),
    ),
    readFile(
      new URL(
        '../../node_modules/@fontsource/geist-mono/files/geist-mono-latin-500-normal.woff2',
        import.meta.url,
      ),
    ),
  ]);
  return [
    { name: 'Bricolage Grotesque', data: bricolage },
    { name: 'Gambetta', weight: 400, data: gambetta },
    { name: 'Geist Mono', weight: 400, data: geist400 },
    { name: 'Geist Mono', weight: 500, data: geist500 },
  ];
}

const esc = (s) =>
  s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');

const labelStyle = `font-family:'Geist Mono';font-weight:500;font-size:15px;letter-spacing:0.32em;text-transform:uppercase;color:${palette.sub}`;

export function ogCard({ line, wordmarkSize = 176, markSize = 300 }) {
  const html =
    `<div style="width:100%;height:100%;display:flex;flex-direction:column;` +
    `background:${palette.ground};padding:72px 76px 52px;font-family:Gambetta,serif;">` +
    `<div style="flex:1;display:flex;align-items:center;justify-content:space-between;gap:72px;min-height:0;">` +
    `<div style="flex:1;display:flex;flex-direction:column;justify-content:center;max-width:640px;">` +
    `<div style="font-family:'Bricolage Grotesque';font-weight:800;font-size:${wordmarkSize}px;line-height:1;letter-spacing:-0.01em;color:${palette.ink};font-variation-settings:'opsz' 96, 'wdth' 88;">` +
    `pith<span style="color:${palette.accent};">.</span></div>` +
    `<p style="font-size:29px;line-height:1.5;color:${palette.sub};margin-top:36px;">${esc(line)}</p>` +
    `</div>` +
    `<img src="og-mark" style="width:${markSize}px;height:${markSize}px;flex:none;"/>` +
    `</div>` +
    `<div style="border-top:1.5px solid ${palette.line};padding-top:24px;">` +
    `<span style="${labelStyle};">pith-lang.org</span>` +
    `</div>` +
    `</div>`;

  return {
    html,
    width: CARD_WIDTH,
    height: CARD_HEIGHT,
    images: [{ src: 'og-mark', data: async () => markSvg({ size: markSize }).buffer }],
  };
}

export async function renderOgCard(card, { fonts } = {}) {
  const { node } = fromHtml(card.html);
  return render(node, {
    width: card.width,
    height: card.height,
    fonts: fonts ?? (await loadOgFonts()),
    images: card.images,
  });
}
