// Says how much room the tab bar takes at the foot of a tab's screen: each
// tab's safe area includes the bar. The floating voice pill (CallSheet)
// rests just above it.
import { useEffect } from 'react';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { setTabInset } from '@/state/actions/ui';

export function TabInsetProbe() {
  const bottom = useSafeAreaInsets().bottom;
  useEffect(() => {
    setTabInset(bottom);
  }, [bottom]);
  return null;
}
