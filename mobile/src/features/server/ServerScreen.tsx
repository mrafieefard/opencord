// Inside a server (spec §7.2): two panes in one sliding row on a phone, a
// fixed sidebar beside the content from 768 pt wide. Back moves one step
// out on a phone and always leaves on a tablet; the stack's swipe back is
// on only where it can't be mistaken for opening the sidebar.
import { useFocusEffect, useNavigation } from 'expo-router';
import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { BackHandler, useWindowDimensions } from 'react-native';

import { PhonePanes } from './PhonePanes';
import { TabletPanes } from './TabletPanes';
import { useServerPanes } from './useServerPanes';

export const TABLET_MIN_WIDTH = 768;

export interface ServerScreenProps {
  serverKey: string;
  /** A channel to open at once (the connected-voice bar, links). */
  channelId?: string;
  /** Opens the sidebar once the screen is up: for screenshots only (the route's `sidebar=1`). */
  startWithSidebar?: boolean;
}

export function ServerScreen({ serverKey, channelId, startWithSidebar = false }: ServerScreenProps) {
  const { width } = useWindowDimensions();
  const tablet = width >= TABLET_MIN_WIDTH;
  const panes = useServerPanes(serverKey, channelId);
  const { mode, back, openSidebar } = panes;
  const navigation = useNavigation();

  useLayoutEffect(() => {
    navigation.setOptions({ gestureEnabled: tablet || mode === 'channels' });
  }, [navigation, tablet, mode]);

  useFocusEffect(
    useCallback(() => {
      if (tablet) return undefined;
      const subscription = BackHandler.addEventListener('hardwareBackPress', back);
      return () => subscription.remove();
    }, [tablet, back]),
  );

  const sidebarAsked = useRef(startWithSidebar);
  useEffect(() => {
    if (!sidebarAsked.current || tablet) return;
    sidebarAsked.current = false;
    openSidebar();
  }, [tablet, openSidebar]);

  return tablet ? (
    <TabletPanes serverKey={serverKey} panes={panes} />
  ) : (
    <PhonePanes serverKey={serverKey} panes={panes} width={width} />
  );
}
