// Says how much room the tab bar takes at the foot of a tab's screen: each
// tab's safe area includes the bar. The floating voice pill (CallSheet)
// rests just above it. While the bar is hidden (a chat in front) the inset
// is only the window's own, and the bar comes back before it grows again:
// that value is never the bar's, so it is kept from the pill, which would
// aim at it and then jump.
import { useEffect } from 'react';
import { initialWindowMetrics, useSafeAreaInsets } from 'react-native-safe-area-context';

import { setTabInset } from '@/state/actions/ui';

export function TabInsetProbe() {
  const bottom = useSafeAreaInsets().bottom;
  useEffect(() => {
    const window = initialWindowMetrics?.insets.bottom ?? 0;
    if (bottom > window + 1) setTabInset(bottom);
  }, [bottom]);
  return null;
}
