/// <reference types="node" />
import { readFileSync } from 'fs';
import { resolve } from 'path';

import { tokens } from '../theme/tokens';

/** `Color(0xAARRGGBB)` as `#RRGGBB`, or `#RRGGBBAA` when not opaque. */
function hex(argb: string): string {
  const alpha = argb.slice(0, 2).toUpperCase();
  const rgb = argb.slice(2).toUpperCase();
  return alpha === 'FF' ? `#${rgb}` : `#${rgb}${alpha}`;
}

function desktop(scheme: 'dark' | 'light'): Record<string, string | string[]> {
  const dart = readFileSync(resolve(__dirname, '../../../../app/lib/ui/theme/oc_colors.dart'), 'utf8');
  const block = dart.split(`static const ${scheme} = OcColors(`)[1]?.split('\n  );')[0] ?? '';
  const values: Record<string, string | string[]> = {};
  for (const [, name, argb] of block.matchAll(/(\w+): Color\(0x([0-9A-Fa-f]{8})\)/g)) {
    if (name && argb) values[name] = hex(argb);
  }
  const shades = block.split('avatarShades: [')[1]?.split(']')[0] ?? '';
  values.avatarShades = [...shades.matchAll(/Color\(0x([0-9A-Fa-f]{8})\)/g)].map((match) => hex(match[1] ?? ''));
  return values;
}

describe('the colors', () => {
  it.each(['dark', 'light'] as const)("tokens match the desktop's OcColors value for value (%s)", (scheme) => {
    const expected = desktop(scheme);
    expect(Object.keys(expected).length).toBeGreaterThan(15);
    expect({ ...tokens[scheme] }).toEqual(expected);
  });
});
