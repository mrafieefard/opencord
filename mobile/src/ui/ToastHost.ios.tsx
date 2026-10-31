// iOS presents a sheet as a modal above the app's root, so toasts drawn
// in the root would sit under it. They go in react-native-screens'
// FullWindowOverlay, over every sheet; it lets touches through, and the
// toasts take none.
import { FullWindowOverlay } from 'react-native-screens';

import { Toasts } from './Toasts';

export function ToastHost() {
  return (
    <FullWindowOverlay>
      <Toasts />
    </FullWindowOverlay>
  );
}
