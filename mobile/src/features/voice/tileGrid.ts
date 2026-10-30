// How a voice channel's people fill its pane, as WhatsApp's and Discord's
// calls: one person takes it all, two stack, more go two across on a phone
// (more on a wider pane), every tile growing to fill the room. At most six
// rows show at once; past that the tiles keep that size and the grid scrolls.

/** Tiles are at most this wide, so a wider pane has more columns: two on a phone, more on a tablet. */
const TILE_MAX = 200;
/** No more rows than this share the pane's height; more scroll. */
export const MAX_ROWS = 6;
/** Nor is a tile ever shorter than this: a short pane shows fewer rows and scrolls. */
const MIN_HEIGHT = 88;
/** The avatar grows with its tile, between these. */
const AVATAR_MIN = 32;
const AVATAR_MAX = 112;
/** What a tile holds besides its avatar: the name below it, the gap, the padding. */
const TILE_CHROME = 44;

export interface TileGrid {
  columns: number;
  tileWidth: number;
  tileHeight: number;
  /** A last tile alone on its row spans it (two columns, an odd count). */
  lastWidth: number;
  avatar: number;
}

/** The most columns a pane this wide takes: two on a phone. */
export function maxColumns(inner: number, gap: number): number {
  return Math.max(2, Math.ceil((inner + gap) / (TILE_MAX + gap)));
}

/**
 * The grid for `count` people in a pane's room (inside its padding).
 * Columns are chosen for the largest tiles, but never one column for
 * three or more when two fit, as the calls it follows.
 */
export function tileGrid(count: number, width: number, height: number, gap: number): TileGrid {
  const most = maxColumns(width, gap);
  const fit = Math.max(1, Math.floor((height + gap) / (MIN_HEIGHT + gap)));
  const sized = (columns: number) => {
    const rows = Math.max(1, Math.min(Math.ceil(count / columns), MAX_ROWS, fit));
    const tileWidth = Math.floor((width - gap * (columns - 1)) / columns);
    const tileHeight = Math.max(MIN_HEIGHT, Math.floor((height - gap * (rows - 1)) / rows));
    return { columns, tileWidth, tileHeight };
  };
  let best = sized(1);
  for (let columns = 2; columns <= Math.min(most, Math.max(count, 1)); columns += 1) {
    const next = sized(columns);
    const bigger = Math.min(next.tileWidth, next.tileHeight) > Math.min(best.tileWidth, best.tileHeight);
    if (bigger || (best.columns === 1 && count >= 3)) best = next;
  }
  const lone = best.columns === 2 && count % 2 === 1 && count > 1;
  const avatar = Math.round(
    Math.min(AVATAR_MAX, Math.max(AVATAR_MIN, Math.min(best.tileWidth * 0.5, best.tileHeight - TILE_CHROME))),
  );
  return { ...best, lastWidth: lone ? width : best.tileWidth, avatar };
}
