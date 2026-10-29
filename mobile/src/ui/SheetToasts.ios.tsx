// iOS presents a sheet as a modal above the app's root, where ToastHost
// draws, so a sheet draws the toasts itself, over its own content.
// (react-native-screens' FullWindowOverlay ends up under the sheet too.)
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { space } from './theme/space';
import { Toasts } from './Toasts';

export function SheetToasts() {
  const insets = useSafeAreaInsets();
  return <Toasts bottom={insets.bottom + space.lg} />;
}
