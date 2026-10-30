// The server screen on a phone (spec §7.2): the channel list and the open
// channel in one sliding row, so they move together like a "slide" drawer.
// In `channels` the list is the window's full width; selecting a channel
// slides it out to the left as the channel slides in (the shift), and from
// then on the list is a sidebar `min(86 % of the width, 360 pt)` wide,
// opened by a swipe right from the left half or the pane's sidebar button.
//
// Gesture Handler's ReanimatedDrawerLayout can't start open (entering a
// server would slide the list in after the stack's push), so this is the
// drawer's own spring on one Pan gesture and shared values:
// - `progress`: 0 when the channel fills the screen, 1 when the list is out;
// - `reach`: how far the list reaches when out, which the channel moves by;
// - `listWidth`: the list's own width. It changes only while the list is out
//   of sight, or under the channel as the sidebar widens into the full list.
import { router } from 'expo-router';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { GestureDetector, Touchable, usePanGesture } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withSpring, type WithSpringConfig } from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

import { chrome } from '@/features/servers/chrome';
import { ConnectedBar } from '@/features/voice/ConnectedBar';
import { setChatInFront, setVoicePageInFront } from '@/state/actions/ui';
import { useTheme, withAlpha } from '@/ui';

import { ChannelList } from './ChannelList';
import { ChannelListHeader } from './ChannelListHeader';
import { ContentPane } from './ContentPane';
import type { ServerPanes } from './useServerPanes';

const SIDEBAR_SHARE = 0.86;
const SIDEBAR_MAX = 360;
const SCRIM_ALPHA = 0.4;
/** ReanimatedDrawerLayout's spring and fling projection (spec §9.3: "Gesture Handler's own spring"). */
const SPRING: WithSpringConfig = { damping: 40, stiffness: 500, mass: 1, overshootClamping: true };
const DRAG_TOSS = 0.05;
/** How far a finger moves sideways before the pan takes over, and up or down before it leaves the touch to scrolling. */
const ACTIVE_OFFSET = 10;
const FAIL_OFFSET_Y = 15;

export const sidebarWidth = (windowWidth: number) => Math.min(windowWidth * SIDEBAR_SHARE, SIDEBAR_MAX);

const leave = () => router.back();

export interface PhonePanesProps {
  serverKey: string;
  panes: ServerPanes;
  width: number;
}

export function PhonePanes({ serverKey, panes, width }: PhonePanesProps) {
  const { colors } = useTheme();
  const { mode, open, sidebarOpen, select, back, openSidebar, closeSidebar } = panes;
  const openChannel = useCallback((channelId: string) => select({ id: channelId }), [select]);
  const full = mode === 'channels';
  const listOut = full || sidebarOpen;
  const narrow = sidebarWidth(width);

  // The tab bar shows everywhere in a server but a text channel in front,
  // where the keyboard and the composer need the room.
  const chatInFront = open?.kind === 'text' && !listOut;
  useEffect(() => {
    setChatInFront(chatInFront);
  }, [chatInFront]);
  // A voice channel's own page needs no voice bar above the tab bar.
  const voicePage = open?.kind === 'voice' && !listOut ? open.id : null;
  useEffect(() => {
    setVoicePageInFront(voicePage === null ? null : { serverKey, channelId: voicePage });
  }, [serverKey, voicePage]);
  useEffect(
    () => () => {
      setChatInFront(false);
      setVoicePageInFront(null);
    },
    [],
  );

  const progress = useSharedValue(listOut ? 1 : 0);
  const reach = useSharedValue(full ? width : narrow);
  const listWidth = useSharedValue(full ? width : narrow);
  const wasOut = useRef(listOut);
  // Where a drag has already sprung to: the state change it hands over
  // must not restart the spring (and lose the fling's speed). A drag that
  // changed nothing leaves a value no later change can match.
  const dragTarget = useRef<boolean | null>(null);

  useEffect(() => {
    const wasListOut = wasOut.current;
    wasOut.current = listOut;
    const dragged = !full && dragTarget.current === listOut;
    dragTarget.current = null;
    if (dragged) return;
    if (full) {
      // From the open sidebar the list widens under the channel as it moves
      // away; from a closed one the list is out of sight.
      listWidth.set(width);
      reach.set(wasListOut ? withSpring(width, SPRING) : width);
      progress.set(withSpring(1, SPRING));
    } else if (listOut) {
      listWidth.set(narrow);
      reach.set(narrow);
      progress.set(withSpring(1, SPRING));
    } else {
      // The shift keeps the list wide until it has slid away.
      progress.set(
        withSpring(0, SPRING, (finished) => {
          if (!finished) return;
          listWidth.set(narrow);
          reach.set(narrow);
        }),
      );
    }
  }, [full, listOut, width, narrow, listWidth, reach, progress]);

  const settleDrag = useCallback(
    (toOpen: boolean) => {
      dragTarget.current = toOpen;
      if (toOpen) openSidebar();
      else closeSidebar();
    },
    [openSidebar, closeSidebar],
  );

  const start = useSharedValue(0);
  const hitSlop = useMemo(() => (sidebarOpen ? undefined : { left: 0, width: width / 2 }), [sidebarOpen, width]);
  const pan = usePanGesture({
    enabled: !full,
    hitSlop,
    activeOffsetX: sidebarOpen ? -ACTIVE_OFFSET : ACTIVE_OFFSET,
    failOffsetY: [-FAIL_OFFSET_Y, FAIL_OFFSET_Y],
    onActivate: () => {
      // Caught mid-shift: the list becomes the sidebar it is about to be.
      listWidth.set(narrow);
      reach.set(narrow);
      start.set(progress.get());
    },
    onUpdate: (event) => {
      progress.set(Math.min(Math.max(start.get() + event.translationX / reach.get(), 0), 1));
    },
    onDeactivate: (event) => {
      const velocity = event.velocityX / reach.get();
      const toOpen = progress.get() + DRAG_TOSS * velocity > 0.5;
      progress.set(withSpring(toOpen ? 1 : 0, { ...SPRING, velocity }));
      scheduleOnRN(settleDrag, toOpen);
    },
  });

  const listStyle = useAnimatedStyle(() => ({
    width: listWidth.get(),
    transform: [{ translateX: (progress.get() - 1) * reach.get() }],
  }));
  const contentStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: progress.get() * reach.get() }],
  }));
  const scrimStyle = useAnimatedStyle(() => ({ opacity: progress.get() }));

  // The sidebar's back goes to the full list: one step out closes the
  // sidebar, a second widens the list into `channels`.
  const backToChannels = useCallback(() => {
    if (sidebarOpen) back();
    back();
  }, [back, sidebarOpen]);

  const covered = !full && sidebarOpen;
  return (
    <GestureDetector gesture={pan}>
      <View style={[styles.root, { backgroundColor: colors.sidebar }]}>
        <Animated.View
          style={[styles.list, listStyle]}
          importantForAccessibility={listOut ? 'auto' : 'no-hide-descendants'}
          accessibilityElementsHidden={!listOut}
        >
          {full ? (
            <ChannelListHeader serverKey={serverKey} backLabel="Servers" onBack={leave} />
          ) : (
            <ChannelListHeader serverKey={serverKey} backLabel="All channels" onBack={backToChannels} />
          )}
          <ChannelList serverKey={serverKey} variant={full ? 'full' : 'sidebar'} selectedId={open?.id} onSelect={select} />
          {/* iOS shows the call above the tab bar instead (chrome.ts). */}
          {chrome.serversVoiceBar && (
            <ConnectedBar serverKey={serverKey} channelId={full ? undefined : open?.id} onOpenChannel={openChannel} bottomInset />
          )}
        </Animated.View>
        <Animated.View style={[styles.content, contentStyle]}>
          <View
            style={styles.fill}
            importantForAccessibility={listOut ? 'no-hide-descendants' : 'auto'}
            accessibilityElementsHidden={listOut}
          >
            <ContentPane
              serverKey={serverKey}
              open={open}
              onOpenSidebar={openSidebar}
              onOpenChannel={openChannel}
              covered={listOut}
            />
          </View>
          <Animated.View
            pointerEvents={covered ? 'auto' : 'none'}
            style={[styles.scrim, { backgroundColor: withAlpha(colors.scrim, SCRIM_ALPHA) }, scrimStyle]}
          >
            <Touchable onPress={closeSidebar} style={styles.fill}>
              <View accessible={covered} accessibilityRole="button" accessibilityLabel="Hide channels" style={styles.fill} />
            </Touchable>
          </Animated.View>
        </Animated.View>
      </View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, overflow: 'hidden' },
  list: { position: 'absolute', top: 0, bottom: 0, left: 0 },
  content: { ...StyleSheet.absoluteFill },
  scrim: { ...StyleSheet.absoluteFill },
  fill: { flex: 1 },
});
