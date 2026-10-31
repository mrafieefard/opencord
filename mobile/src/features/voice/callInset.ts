// Android: the open call fills the screen's width, to its bottom edge, with
// rounded top corners as before (callInset.ios.ts floats it, as Apple Maps').
export const CALL_MARGIN = 0;

/** Open, its bottom corners are square: it stands on the screen's bottom edge. */
export const OPEN_BOTTOM_ROUND = false;

export function openCorner(_width: number, _height: number): number {
  return 38;
}
