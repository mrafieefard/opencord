// iOS: the open call floats as Apple Maps' card does, inset from the
// screen's sides and bottom, its corners following the screen's own curve
// (concentric with it: the screen's radius less the inset).
import { screenCorner } from './screenCorner';

export const CALL_MARGIN = 8;

/** The open call's corner radius on a screen this size. */
export function openCorner(width: number, height: number): number {
  return Math.max(screenCorner(width, height) - CALL_MARGIN, 16);
}
