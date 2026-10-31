// How round an iPhone's or iPad's screen corners are, in points, by its
// size: UIKit has no public way to ask. Sheets that float above the screen's
// edge follow this curve, as Apple's own do.

/** Portrait width and height, and the corner radius, of each screen with round corners. */
const SCREENS: readonly (readonly [number, number, number])[] = [
  [375, 812, 39], // X, XS, 11 Pro
  [360, 780, 44], // 12 mini, 13 mini
  [414, 896, 41.5], // XR, 11, XS Max, 11 Pro Max
  [390, 844, 47.33], // 12, 13, 14, 16e
  [428, 926, 53.33], // 12 Pro Max, 13 Pro Max, 14 Plus
  [393, 852, 55], // 14 Pro, 15, 15 Pro, 16
  [430, 932, 55], // 14 Pro Max, 15 Plus, 15 Pro Max, 16 Plus
  [402, 874, 62], // 16 Pro, 17, 17 Pro
  [440, 956, 62], // 16 Pro Max, 17 Pro Max
  [420, 912, 62], // Air
];

/** An iPad's corners; a phone not listed with a tall screen gets a phone's. */
const TABLET = 18;
const TALL_PHONE = 44;

export function screenCorner(width: number, height: number): number {
  const short = Math.round(Math.min(width, height));
  const long = Math.round(Math.max(width, height));
  const known = SCREENS.find(([w, h]) => w === short && h === long);
  if (known) return known[2];
  // Home-button phones (16:9) have square corners; anything wider is a tablet.
  if (long / short > 2) return TALL_PHONE;
  if (short >= 600) return TABLET;
  return 0;
}
