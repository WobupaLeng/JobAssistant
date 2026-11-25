import { describe, expect, it } from 'vitest';
import { PALETTES, findPalette } from '../src/lib_ui/palette';

describe('palette catalog', () => {
  it('offers seven palettes with unique ids and labels', () => {
    expect(PALETTES).toHaveLength(7);
    expect(new Set(PALETTES.map((p) => p.id)).size).toBe(7);
    expect(new Set(PALETTES.map((p) => p.label)).size).toBe(7);
    for (const p of PALETTES) {
      expect(p.swatch).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it('defines a complete tone pair for every palette', () => {
    for (const p of PALETTES) {
      for (const tone of [p.dark, p.light]) {
        expect(Object.keys(tone)).toHaveLength(9);
        for (const value of Object.values(tone)) {
          expect(value).toMatch(/\S/);
        }
      }
    }
  });

  it('looks up new palettes and falls back to cobalt', () => {
    expect(findPalette('coral').id).toBe('coral');
    expect(findPalette('moss').id).toBe('moss');
    expect(findPalette('nope').id).toBe('cobalt');
    expect(findPalette('emerald').id).toBe('cobalt');
  });
});
