// iOS: the open call fills the screen, as Apple Maps' card does at its full
// height: the whole width, down to the bottom edge, its corners the
// screen's own curve, so nothing of the screen behind shows at its sides.
import { screenCorner } from './screenCorner';

export const CALL_MARGIN = 0;

/** Open, its bottom corners are round too: the screen's own. */
export const OPEN_BOTTOM_ROUND = true;

/** Sheets are presented natively, above the call, which stays open under them. */
export const SHEETS_ABOVE_CALL = true;

/** The open call's corner radius on a screen this size: the screen's. */
export function openCorner(width: number, height: number): number {
  return Math.max(screenCorner(width, height) - CALL_MARGIN, 16);
}
