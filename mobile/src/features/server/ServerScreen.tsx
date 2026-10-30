// Inside a server (spec §7.2): two panes in one sliding row on a phone, a
// fixed sidebar beside the content from 768 pt wide. Back moves one step
// out on a phone and always leaves on a tablet; the stack's swipe back is
// on only where it can't be mistaken for opening the sidebar. A voice
// channel opens as a sheet to join it, as Discord's; once this device is
// in it, the call opens instead (openCall).
import { ChannelKind, type ChannelRecord } from '@opencord/core-native';
import { router, useFocusEffect, useNavigation } from 'expo-router';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import { BackHandler, useWindowDimensions } from 'react-native';

import { openCall } from '@/features/voice/openCall';
import { useAppStore } from '@/state/store';

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
  const opened = useServerPanes(serverKey, channelId);
  const { mode, back, openSidebar } = opened;
  const panes = useMemo(
    () => ({
      ...opened,
      select: (channel: Pick<ChannelRecord, 'id'> & Partial<Pick<ChannelRecord, 'kind'>>) => {
        const mine = useAppStore.getState().myVoice;
        const inIt = mine?.serverKey === serverKey && mine.channelId === channel.id;
        if (channel.kind === ChannelKind.Voice) {
          // Not in it: the sheet to join it. In it: the call.
          if (inIt) openCall(serverKey, channel.id);
          else router.push(`/server/${serverKey}/voice/${channel.id}`);
          return;
        }
        opened.select(channel);
      },
    }),
    [opened, serverKey],
  );
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
