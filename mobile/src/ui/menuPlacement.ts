// Where the Android menu goes (Menu.android.tsx): below what was pressed,
// or above it when there's no room below, kept inside the screen.

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Bounds {
  width: number;
  height: number;
  /** The system bars. */
  top: number;
  bottom: number;
}

export interface Placement {
  left: number;
  top: number;
  /** The corner the menu grows from. */
  origin: `${'top' | 'bottom'} ${'left' | 'right'}`;
}

/** From the screen's edges and from the anchor. */
export const MENU_MARGIN = 8;
/** An anchor at least this share of the screen wide is a row, not a button or a bubble. */
const WIDE = 0.9;
/** Where a menu starts on a row: the rows' own inset. */
const ROW_INSET = 16;

export function placeMenu(anchor: Rect, menu: { width: number; height: number }, bounds: Bounds): Placement {
  const maxLeft = bounds.width - MENU_MARGIN - menu.width;
  let left: number;
  let side: 'left' | 'right';
  if (anchor.width >= bounds.width * WIDE) {
    left = anchor.x + ROW_INSET;
    side = 'left';
  } else if (anchor.x + anchor.width / 2 > bounds.width / 2) {
    left = anchor.x + anchor.width - menu.width;
    side = 'right';
  } else {
    left = anchor.x;
    side = 'left';
  }
  left = Math.min(Math.max(left, MENU_MARGIN), maxLeft);

  const lowest = bounds.height - bounds.bottom - MENU_MARGIN;
  const highest = bounds.top + MENU_MARGIN;
  const below = anchor.y + anchor.height + MENU_MARGIN;
  const above = anchor.y - MENU_MARGIN - menu.height;
  if (below + menu.height <= lowest) return { left, top: below, origin: `top ${side}` };
  if (above >= highest) return { left, top: above, origin: `bottom ${side}` };
  // A tall anchor with no room either side: over it, as low as fits.
  return { left, top: Math.max(highest, lowest - menu.height), origin: `bottom ${side}` };
}
