// Android's header draws nothing in a system style of its own: nothing to set.
import type { Scheme } from './theme/tokens';

export function nativeHeaderScheme(_scheme: Scheme): { headerUserInterfaceStyle?: Scheme } {
  return {};
}
