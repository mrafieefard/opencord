// A sheet that fits its content ends above the home indicator: UIKit adds
// the bottom safe area to the height the content asks for, so padding for
// it again leaves a band of blank sheet under the last control.

/** What a `fitToContents` sheet's content pads at its bottom for the system. */
export function useSheetBottomInset(): number {
  return 0;
}
