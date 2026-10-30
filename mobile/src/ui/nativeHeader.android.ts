// Android's header is drawn in the screen's colour (nativeHeader.ios.ts
// leaves iOS's to the system's glass).
import type { Scheme } from './theme/tokens';

export interface NativeHeaderLook {
  headerUserInterfaceStyle?: Scheme;
  headerStyle?: { backgroundColor: string };
  headerLargeStyle?: { backgroundColor: string };
}

export function nativeHeaderLook(_scheme: Scheme, background: string): NativeHeaderLook {
  return { headerStyle: { backgroundColor: background }, headerLargeStyle: { backgroundColor: background } };
}
