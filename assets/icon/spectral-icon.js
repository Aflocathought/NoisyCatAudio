/* Shared by the preview page and Node exporter. No runtime dependencies. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.SpectralIcon = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  // Keep these aligned with BLUE / WHITE / INK in src/editor.rs.
  const palette = Object.freeze({ blue: '#3593FF', white: '#E8F2FF', ink: '#080F1A' });
  const defaults = Object.freeze({ layers: 4, spacing: 34, weight: 8, background: 'dark' });

  function mix(a, b, amount) {
    const rgb = value => value.match(/[0-9a-f]{2}/gi).map(part => parseInt(part, 16));
    const left = rgb(a), right = rgb(b);
    return '#' + left.map((value, i) => Math.round(value + (right[i] - value) * amount)
      .toString(16).padStart(2, '0')).join('');
  }

  function bounded(value, fallback, min, max) {
    const number = Number(value ?? fallback);
    return Number.isFinite(number) ? Math.max(min, Math.min(max, number)) : fallback;
  }

  function svg(options = {}) {
    const layers = Math.round(bounded(options.layers, defaults.layers, 2, 5));
    const spacing = bounded(options.spacing, defaults.spacing, 20, 40);
    const weight = bounded(options.weight, defaults.weight, 5, 12);
    const size = Math.round(bounded(options.size, 512, 16, 4096));
    const background = options.background === 'transparent' ? 'transparent' : 'dark';
    // Namespaced IDs allow several inline icons in one page without gradient collisions.
    const id = String(options.id ?? 'spectral').replace(/[^a-zA-Z0-9_-]/g, '') || 'spectral';
    const n = value => value.toFixed(2);
    const gaussian = (x, center, width) => Math.exp(-0.5 * ((x - center) / width) ** 2);

    // This is a stylized frequency response, not a time-domain sine wave.
    // Wide blue excitation supports increasingly narrow and bright resonances.
    function curve(layer) {
      const base = 395 - layer * spacing;
      const points = [];
      for (let i = 0; i <= 192; i++) {
        const t = i / 192;
        const x = 66 + t * 380;
        const taper = Math.sin(Math.PI * t) ** 0.32;
        const width = layer === 0 ? 27 : 21 - layer * 0.9;
        const height = 135 + layer * 4;
        let amplitude = 0.53 * gaussian(x, 154, width * 0.85)
          + gaussian(x, 252, width)
          + 0.65 * gaussian(x, 358, width * 0.9);
        if (layer === 0) {
          amplitude += 0.15 * gaussian(x, 104, 10) + 0.11 * gaussian(x, 201, 9)
            + 0.12 * gaussian(x, 307, 10) + 0.13 * gaussian(x, 410, 10);
        }
        points.push([x, base - height * amplitude * taper]);
      }
      return { base, line: points.map(([x, y], index) => `${index ? 'L' : 'M'}${n(x)} ${n(y)}`).join(' ') };
    }

    // A fixed vertical envelope keeps all supported layer/spacing combinations inside
    // the same optical bounds. It also leaves safe padding for small app icons.
    const top = 395 - layers * spacing - (135 + layers * 4);
    const scaleY = 296 / (417 - top);
    const shiftY = 110 - top * scaleY;
    const base = curve(0);
    const paths = [
      `<path d="${base.line} L446 404 Q446 417 433 417 H79 Q66 417 66 404 Z" fill="url(#${id}-body)"/>`,
      `<path d="${base.line}" fill="none" stroke="${palette.blue}" stroke-width="${weight + 1}"/>`,
    ];
    for (let layer = 1; layer <= layers; layer++) {
      const response = curve(layer);
      const whiteness = 0.24 + 0.76 * layer / layers;
      const color = mix(palette.blue, palette.white, whiteness);
      paths.push(`<path d="${response.line} L446 ${response.base + 10} H66 Z" fill="${color}" fill-opacity="0.035"/>`);
      paths.push(`<path d="${response.line}" fill="none" stroke="${color}" stroke-width="${weight}"/>`);
    }
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 512 512" fill="none" role="img" aria-labelledby="${id}-title">
  <title id="${id}-title">Specatral Resonator — layered resonance</title>
  <defs>
    <linearGradient id="${id}-body" x1="256" y1="244" x2="256" y2="417" gradientUnits="userSpaceOnUse">
      <stop stop-color="${palette.blue}" stop-opacity="0.9"/>
      <stop offset="1" stop-color="${palette.blue}" stop-opacity="0.14"/>
    </linearGradient>
  </defs>
  ${background === 'dark' ? `<rect width="512" height="512" rx="108" fill="${palette.ink}"/>` : ''}
  <g transform="translate(0 ${n(shiftY)}) scale(1 ${n(scaleY)})" stroke-linecap="round" stroke-linejoin="round">
    ${paths.join('\n    ')}
  </g>
</svg>`.replace(/[ \t]+$/gm, '');
  }

  return Object.freeze({ svg, palette, defaults });
});
