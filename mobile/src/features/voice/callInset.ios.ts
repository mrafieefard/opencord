// iOS: the open call floats as Apple Maps' card does, inset from the
// screen's sides and bottom, its corners following the screen's own curve
// (concentric with it: the screen's radius less the inset).
import { screenCorner } from './screenCorner';

export const CALL_MARGIN = 8;

/** Open, its bottom corners are round too, floating above the screen's edge. */
export const OPEN_BOTTOM_ROUND = true;

/** Sheets are presented natively, above the call, which stays open under them. */
export const SHEETS_ABOVE_CALL = true;

/** The open call's corner radius on a screen this size. */
export function openCorner(width: number, height: number): number {
  return Math.max(screenCorner(width, height) - CALL_MARGIN, 16);
}
