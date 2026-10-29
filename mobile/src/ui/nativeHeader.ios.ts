// UIKit draws a native header's own parts (the search field, its glass) in
// the style the header is given. The stack takes that from React
// Navigation's theme, which the app doesn't set, so without this a dark
// header gets a light search field: grey text on near-black.
import type { Scheme } from './theme/tokens';

export function nativeHeaderScheme(scheme: Scheme): { headerUserInterfaceStyle?: Scheme } {
  return { headerUserInterfaceStyle: scheme };
}
