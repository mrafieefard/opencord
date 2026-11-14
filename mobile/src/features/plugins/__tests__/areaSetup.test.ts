import { tokens } from '@/ui/theme/tokens';

import { areaSetup } from '../areaSetup';

const darkColors = tokens.dark;
const lightColors = tokens.light;

// The twelve names the desktop's buildBlocksTheme gives BlocksTheme.
const NAMES = [
  'text',
  'text_secondary',
  'text_muted',
  'background',
  'surface',
  'elevated',
  'border',
  'accent',
  'on_accent',
  'selected',
  'hover',
  'danger',
];

describe('areaSetup', () => {
  // Fails if a colour name is missing or not the phone theme's value, or a
  // field of "The plugin area channel"'s setup is misspelled or dropped.
  it('carries the phone theme, insets and scale in the wire shape', () => {
    const json = areaSetup({
      colors: darkColors,
      scheme: 'dark',
      insets: { top: 0, right: 0, bottom: 300, left: 0 },
      textScale: 1.3,
      reduceMotion: true,
      serverKey: 'night-owls',
      pluginId: 'tickets',
    });
    const setup = JSON.parse(json) as Record<string, unknown>;
    expect(setup).toEqual({
      theme: 'dark',
      colors: expect.any(Object),
      text_scale: 1.3,
      safe_area: { top: 0, right: 0, bottom: 300, left: 0 },
      reduce_motion: true,
      server_key: 'night-owls',
      plugin_id: 'tickets',
    });
    const colors = setup.colors as Record<string, string>;
    expect(Object.keys(colors).sort()).toEqual([...NAMES].sort());
    expect(colors.text).toBe(darkColors.text);
    expect(colors.background).toBe(darkColors.chat);
    expect(colors.danger).toBe(darkColors.recording);
    for (const value of Object.values(colors)) expect(value).toMatch(/^#([0-9A-Fa-f]{6}|[0-9A-Fa-f]{8})$/);
  });

  it('light is light', () => {
    const setup = JSON.parse(
      areaSetup({ colors: lightColors, scheme: 'light', insets: { top: 0, right: 0, bottom: 0, left: 0 }, textScale: 1, reduceMotion: false, serverKey: 'k', pluginId: 'p' }),
    ) as { theme: string; colors: Record<string, string> };
    expect(setup.theme).toBe('light');
    expect(setup.colors.text).toBe(lightColors.text);
  });
});
