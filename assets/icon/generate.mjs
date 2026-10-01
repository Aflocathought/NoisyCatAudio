/** Rebuild the vector assets with Node. Add --png for raster exports via sharp. */
import { createRequire } from 'node:module';
import { writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import icon from './spectral-icon.js';

const output = path.dirname(fileURLToPath(import.meta.url));
for (const background of ['dark', 'transparent']) {
  const name = background === 'dark' ? 'spectral-resonator' : 'spectral-resonator-transparent';
  await writeFile(path.join(output, `${name}.svg`), icon.svg({ background }) + '\n');
  console.log(`Generated ${name}.svg`);
}

if (process.argv.includes('--png')) {
  // Optional build-time dependency; browser PNG export needs no Node packages.
  const require = createRequire(import.meta.url);
  const sharp = require(process.env.SPECTRAL_SHARP_PATH || 'sharp');
  for (const background of ['dark', 'transparent']) {
    const name = background === 'dark' ? 'spectral-resonator' : 'spectral-resonator-transparent';
    for (const size of [32, 64, 128, 256, 512, 1024]) {
      await sharp(Buffer.from(icon.svg({ background, size }))).png()
        .toFile(path.join(output, `${name}-${size}.png`));
    }
  }
  // A compact contact sheet makes the default and actual small-size output reviewable.
  const sheet = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="760" viewBox="0 0 1200 760">
    <rect width="1200" height="760" fill="#0C1420"/>
    <text x="64" y="72" fill="#E8F2FF" font-family="Arial,sans-serif" font-size="24" letter-spacing="3">SPECATRAL RESONATOR</text>
    <text x="64" y="102" fill="#899BB0" font-family="Arial,sans-serif" font-size="14" letter-spacing="2">LAYERED RESONANCE / ICON STUDY</text>
    <svg x="64" y="156" width="480" height="480" viewBox="0 0 512 512">${icon.svg({ id: 'hero' }).replace(/^<svg[^>]*>|<\/svg>$/g, '')}</svg>
    <svg x="666" y="178" width="300" height="300" viewBox="0 0 512 512">${icon.svg({ id: 'mark', background: 'transparent' }).replace(/^<svg[^>]*>|<\/svg>$/g, '')}</svg>
    <text x="684" y="546" fill="#899BB0" font-family="Arial,sans-serif" font-size="12" letter-spacing="2">128 / 64 / 32 PX</text>
    ${[128, 64, 32].map((size, i) => `<svg x="${684 + [0, 158, 252][i]}" y="${582 + (128 - size) / 2}" width="${size}" height="${size}" viewBox="0 0 512 512">${icon.svg({ id: 'small' + size }).replace(/^<svg[^>]*>|<\/svg>$/g, '')}</svg>`).join('')}
    <text x="64" y="702" fill="#3593FF" font-family="Arial,sans-serif" font-size="14">#3593FF</text>
    <text x="188" y="702" fill="#E8F2FF" font-family="Arial,sans-serif" font-size="14">#E8F2FF</text>
    <text x="312" y="702" fill="#899BB0" font-family="Arial,sans-serif" font-size="14">#080F1A</text>
  </svg>`;
  await sharp(Buffer.from(sheet)).png().toFile(path.join(output, 'preview.png'));
  console.log('Generated PNG sizes 32–1024 and preview.png');
}
