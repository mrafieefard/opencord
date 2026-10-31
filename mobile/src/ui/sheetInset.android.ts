// Android's sheet reaches the screen's bottom edge, under the navigation bar.
import { useSafeAreaInsets } from 'react-native-safe-area-context';

/** What a `fitToContents` sheet's content pads at its bottom for the system. */
export function useSheetBottomInset(): number {
  return useSafeAreaInsets().bottom;
}
