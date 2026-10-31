// A native header on iOS 26 is glass over the screen: it takes no colour
// of its own (a large title's bar given a background colour hides the
// title), only the app's scheme, which its own parts (the search field,
// the glass) follow. The stack takes that scheme from React Navigation's
// theme otherwise.
import type { Scheme } from './theme/tokens';

export interface NativeHeaderLook {
  headerUserInterfaceStyle?: Scheme;
  headerStyle?: { backgroundColor: string };
  headerLargeStyle?: { backgroundColor: string };
}

export function nativeHeaderLook(scheme: Scheme, _background: string): NativeHeaderLook {
  return { headerUserInterfaceStyle: scheme };
}
