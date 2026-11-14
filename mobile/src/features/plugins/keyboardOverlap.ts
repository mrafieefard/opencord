// How much of a plugin area the keyboard covers, from where the area ends on
// screen: the area is padded by it, so the plugin's sheets and fields sit
// above the keyboard. React Native's KeyboardAvoidingView measures its own
// place from its parent, which under-lifts an area below a header.

export interface OverlapInput {
  /** The area's bottom edge on screen, in points. */
  areaBottom: number;
  screenHeight: number;
  /** 0 while the keyboard is down. */
  keyboardHeight: number;
}

export function keyboardOverlap({ areaBottom, screenHeight, keyboardHeight }: OverlapInput): number {
  if (keyboardHeight <= 0) return 0;
  return Math.max(0, Math.round(areaBottom - (screenHeight - keyboardHeight)));
}
