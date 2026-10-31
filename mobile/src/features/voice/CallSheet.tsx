// The call sheet (callPosition.ts): the voice bar's own glass grown into the
// call, as Apple Maps' card grows. Its frame blends from the bar's to the
// full screen with one shared position: a bar pulled up stretches under the
// finger, and here, dragging the grabber or the header down shrinks it back
// the same way; letting go springs it open or back into the bar. The bar's
// row fades out as the call fades in. The dimmed screen above, the chevron,
// Android's back, or leaving the call put it away. The music player and the
// soundboard open over it as sheets.
//
// On iOS (chrome.floatingVoice) the bar is this glass itself: at rest it is
// the voice pill, floating above the tab bar or in the room a chat keeps for
// it above the composer, and moving between them. So what grows is the pill
// the finger is on, not a copy laid over it. On Android it grows out of the
// app's own bars (ConnectedBar), and shows only while it is out of them.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { BackHandler, Platform, Pressable, StyleSheet, View, useWindowDimensions } from 'react-native';
import { GestureDetector } from 'react-native-gesture-handler';
import { useKeyboardState } from 'react-native-keyboard-controller';
import Animated, {
  Easing,
  interpolate,
  useAnimatedReaction,
  useAnimatedStyle,
  useDerivedValue,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

import { chrome } from '@/features/servers/chrome';
import { useMyVoice } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { GlassSurface, space, useTheme } from '@/ui';

import {
  BAR_HEIGHT,
  type CallFrame,
  PILL_GAP,
  callDrag,
  callOrigin,
  callOriginRound,
  callProgress,
  closeCall,
  sidebarOut,
  slotRise,
  useCallTop,
  useFallbackOrigin,
} from './callPosition';
import { CALL_MARGIN, OPEN_BOTTOM_ROUND, openCorner } from './callInset';
import { ConnectedVoiceRow } from './ConnectedBar';
import { VoicePane } from './VoicePane';

const SCRIM = 0.35;
/**
 * Growing, the glass fills in as Apple Maps' card does, solid once open.
 * Android's surface is solid already.
 */
const FILLS = Platform.OS === 'ios';
const FILL_FROM = 0.1;
const FILL_TO = 0.7;

/** The glass with its corners animated on the glass itself: clipped by a parent, its rim would be cut off. */
const AnimatedGlass = Animated.createAnimatedComponent(GlassSurface);
const GRABBER = { width: 36, height: 5 };

/** The floating pill above the tab bar: inset as the bar's own sides, a little above it. */
const TAB_SIDE = 20;
/** In a chat, the room the composer's dock keeps for it (ChatVoicePill.ios). */
const CHAT_SIDE = space.sm;
/** Moving between its places, and showing or hiding. */
const MOVE_SPRING = { damping: 30, stiffness: 320 };
const FADE_MS = 160;
/** Joining and leaving: the pill fades in or out where it rests, as UIKit's own do. */
const PRESENCE_FADE = { duration: 200, easing: Easing.inOut(Easing.ease) };

const NO_FRAME: CallFrame = { x: 0, y: 0, width: 0, height: BAR_HEIGHT };

/** A frame `at` of the way from one to another. */
function between(from: CallFrame, to: CallFrame, at: number): CallFrame {
  'worklet';
  return {
    x: from.x + (to.x - from.x) * at,
    y: from.y + (to.y - from.y) * at,
    width: from.width + (to.width - from.width) * at,
    height: from.height + (to.height - from.height) * at,
  };
}

/** Where the floating pill rests; in a chat, also the tab bar's place it moves to with the sidebar. */
interface Rest {
  frame: CallFrame;
  /** In a chat: above the tab bar, where the sidebar coming out takes it. */
  tabs: CallFrame | null;
}

/**
 * Where the floating pill rests (iOS), if anywhere now: in a chat's room
 * for it (or, as the sidebar comes out over the chat, on its way above the
 * tab bar), else above the tab bar while that shows. Not while typing, nor
 * over the call's own page. Both places are known before they are needed:
 * the tab bar's from any tab, the chat's from the last chat (ChatVoicePill).
 */
function useRest(
  call: { serverKey: string; channelId: string } | null,
  screenWidth: number,
  screenHeight: number,
): Rest | null {
  const slot = useAppStore((state) => state.voiceSlot);
  const chatInFront = useAppStore((state) => state.chatInFront);
  const tabInset = useAppStore((state) => state.tabInset);
  const onItsPage = useAppStore(
    (state) =>
      state.voicePageInFront !== null &&
      state.voicePageInFront.serverKey === call?.serverKey &&
      state.voicePageInFront.channelId === call.channelId,
  );
  const typing = useKeyboardState((state) => state.isVisible);
  const place =
    !chrome.floatingVoice || !call || onItsPage || typing
      ? 'none'
      : slot
        ? 'chat'
        : chatInFront || tabInset === 0
          ? 'none'
          : 'tabs';
  const slotY = slot?.y ?? 0;
  const slotHeight = slot?.height ?? 0;
  return useMemo(() => {
    const tabs =
      tabInset > 0
        ? {
            x: TAB_SIDE,
            y: screenHeight - tabInset - PILL_GAP - BAR_HEIGHT,
            width: screenWidth - TAB_SIDE * 2,
            height: BAR_HEIGHT,
          }
        : null;
    if (place === 'chat') {
      return { frame: { x: CHAT_SIDE, y: slotY, width: screenWidth - CHAT_SIDE * 2, height: slotHeight }, tabs };
    }
    if (place === 'tabs' && tabs) return { frame: tabs, tabs: null };
    return null;
  }, [place, slotY, slotHeight, screenWidth, screenHeight, tabInset]);
}

export function CallSheet() {
  const { colors } = useTheme();
  const { width: screenWidth, height: screenHeight } = useWindowDimensions();
  const top = useCallTop();
  const fallback = useFallbackOrigin();
  // The call is this device's: there is one at most. Mounted for as long
  // as it lasts, so the glass is there the moment a bar is pulled, and a
  // little longer: left, it is held while the pill slides away.
  const myVoice = useMyVoice();
  const inIt = myVoice !== null;
  const [held, setHeld] = useState(myVoice);
  if (myVoice !== null && myVoice !== held) setHeld(myVoice);
  const call = myVoice ?? held;
  const release = useCallback(() => setHeld(null), []);
  // Joining, the pill fades in where it rests; leaving, it fades out there.
  const presence = useSharedValue(0);
  useEffect(() => {
    if (inIt) {
      presence.set(withTiming(1, PRESENCE_FADE));
    } else {
      presence.set(
        withTiming(0, PRESENCE_FADE, (done) => {
          if (done) scheduleOnRN(release);
        }),
      );
    }
  }, [inIt, presence, release]);
  // Out of the bar, however little: the call takes the touches, not the pill.
  const [up, setUp] = useState(false);
  useAnimatedReaction(
    () => callProgress.get() > 0.001,
    (now, before) => {
      if (now !== before) scheduleOnRN(setUp, now);
    },
  );

  // The floating pill (iOS): where it rests, moving there as its place
  // moves, and fading in or out where it is as it comes or goes.
  // The call held while leaving keeps the pill where it was as it goes.
  const rest = useRest(call, screenWidth, screenHeight);
  const moveFrom = useSharedValue<CallFrame>(rest?.frame ?? NO_FRAME);
  const moveTo = useSharedValue<CallFrame>(rest?.frame ?? NO_FRAME);
  const move = useSharedValue(1);
  // In a chat, the tab bar's place the sidebar takes it to (blends: 1).
  const tabsTo = useSharedValue<CallFrame>(rest?.tabs ?? NO_FRAME);
  const blends = useSharedValue(rest?.tabs ? 1 : 0);
  // Where it is now: on its way from where it was to its place, which in a
  // chat is itself on the way to the tab bar as far as the sidebar is out.
  const placed = useCallback((): CallFrame => {
    'worklet';
    const to = blends.get() ? between(moveTo.get(), tabsTo.get(), sidebarOut.get()) : moveTo.get();
    return between(moveFrom.get(), to, move.get());
  }, [moveFrom, moveTo, tabsTo, blends, move]);
  const shown = useSharedValue(rest ? 1 : 0);
  const wasShown = useRef(rest !== null);
  // Gone only once it has faded: without a place for a moment (between the
  // sidebar and a chat), it keeps where it was and moves from there.
  const forget = useCallback(() => {
    wasShown.current = false;
  }, []);
  useEffect(() => {
    if (!rest) {
      shown.set(
        withTiming(0, { duration: FADE_MS }, (done) => {
          if (done) scheduleOnRN(forget);
        }),
      );
      return;
    }
    // A drag measures its travel from here.
    callOrigin.set(rest.frame);
    if (wasShown.current) {
      // From wherever it is now, mid-move or not.
      moveFrom.set(placed());
      moveTo.set(rest.frame);
      tabsTo.set(rest.tabs ?? rest.frame);
      blends.set(rest.tabs ? 1 : 0);
      move.set(0);
      move.set(withSpring(1, MOVE_SPRING));
    } else {
      moveFrom.set(rest.frame);
      moveTo.set(rest.frame);
      tabsTo.set(rest.tabs ?? rest.frame);
      blends.set(rest.tabs ? 1 : 0);
      move.set(1);
    }
    wasShown.current = true;
    shown.set(withTiming(1, { duration: FADE_MS }));
  }, [rest, moveFrom, moveTo, tabsTo, blends, move, shown, forget, placed]);
  const resting = rest !== null;

  // Leaving the call puts it away.
  useEffect(() => {
    if (!inIt) closeCall();
  }, [inIt]);

  useEffect(() => {
    if (!up) return undefined;
    const subscription = BackHandler.addEventListener('hardwareBackPress', () => {
      closeCall();
      return true;
    });
    return () => subscription.remove();
  }, [up]);

  const grabberDrag = useMemo(() => callDrag(top, screenHeight), [top, screenHeight]);
  const headerDrag = useMemo(() => callDrag(top, screenHeight), [top, screenHeight]);
  // Open, it floats inset from the sides and bottom (iOS), its corners
  // following the screen's.
  const openWidth = screenWidth - CALL_MARGIN * 2;
  const openHeight = screenHeight - top - CALL_MARGIN;
  const corner = openCorner(screenWidth, screenHeight);

  // Where the glass grows from: the floating pill's place (risen with a
  // chat's field), or the bar last touched.
  // Every shared value it follows is read here, directly: Reanimated
  // re-runs a worklet for the shared values it names itself, not for those a
  // function it calls reads, so through a helper the pill moved only when a
  // React render happened to come by, and jumped there.
  const origin = useDerivedValue((): CallFrame => {
    if (chrome.floatingVoice) {
      const out = sidebarOut.get();
      const chat = blends.get();
      const to = chat ? between(moveTo.get(), tabsTo.get(), out) : moveTo.get();
      const at = between(moveFrom.get(), to, move.get());
      // Risen with the chat's field, less as the sidebar takes it away.
      return { ...at, y: at.y - slotRise.get() * (1 - chat * out) };
    }
    const touched = callOrigin.get();
    return touched.width > 0 ? touched : fallback;
  });

  // The glass's frame: from the bar's to the open call's.
  const frameStyle = useAnimatedStyle(() => {
    const at = callProgress.get();
    const from = origin.get();
    return {
      opacity: Math.max(shown.get(), at > 0.001 ? 1 : 0) * presence.get(),
      top: from.y + (top - from.y) * at,
      left: from.x + (CALL_MARGIN - from.x) * at,
      width: from.width + (openWidth - from.width) * at,
      height: from.height + (openHeight - from.height) * at,
    };
  });
  // Its corners: from the bar's (a pill's round ends, Android's square
  // bar's none) to the screen's own, on the glass and on what it holds alike.
  // On Android, standing on the screen's bottom edge, the bottom ones square.
  const cornerStyle = useAnimatedStyle(() => {
    const from = origin.get();
    const at = callProgress.get();
    const start = chrome.floatingVoice || callOriginRound.get() ? from.height / 2 : 0;
    const top = start + (corner - start) * at;
    if (OPEN_BOTTOM_ROUND) return { borderRadius: top };
    const bottom = start * (1 - at);
    return {
      borderTopLeftRadius: top,
      borderTopRightRadius: top,
      borderBottomLeftRadius: bottom,
      borderBottomRightRadius: bottom,
    };
  });
  const fillStyle = useAnimatedStyle(() => ({
    opacity: interpolate(callProgress.get(), [FILL_FROM, FILL_TO], [0, 1], 'clamp'),
  }));
  const scrimStyle = useAnimatedStyle(() => ({ opacity: callProgress.get() * SCRIM }));
  // The bar's row, as it was, fading as the glass leaves it behind.
  const rowStyle = useAnimatedStyle(() => {
    const from = origin.get();
    return {
      width: from.width,
      height: from.height,
      opacity: interpolate(callProgress.get(), [0, 0.2], [1, 0], 'clamp'),
    };
  });
  const callStyle = useAnimatedStyle(() => ({ opacity: interpolate(callProgress.get(), [0.15, 0.55], [0, 1], 'clamp') }));

  if (!call) return null;
  return (
    <View style={StyleSheet.absoluteFill} pointerEvents={up || resting ? 'box-none' : 'none'}>
      <Animated.View
        style={[StyleSheet.absoluteFill, { backgroundColor: colors.scrim }, scrimStyle]}
        pointerEvents={up ? 'auto' : 'none'}
      >
        <Pressable style={StyleSheet.absoluteFill} onPress={closeCall} accessibilityLabel="Close the call" />
      </Animated.View>
      <Animated.View
        style={[styles.frame, frameStyle]}
        accessibilityViewIsModal={up}
        // Out of sight, out of VoiceOver's reach too.
        accessibilityElementsHidden={!up && !resting}
        importantForAccessibility={up || resting ? 'auto' : 'no-hide-descendants'}
        testID="call-sheet"
      >
        <AnimatedGlass style={[StyleSheet.absoluteFill, cornerStyle]} />
        <Animated.View style={[StyleSheet.absoluteFill, styles.clip, cornerStyle]} pointerEvents="box-none">
          {FILLS && (
            <Animated.View
              style={[StyleSheet.absoluteFill, { backgroundColor: colors.chat }, fillStyle]}
              pointerEvents="none"
            />
          )}
          <Animated.View
            style={[styles.row, rowStyle]}
            // At rest, the pill's own row: tapped, pulled, muted. On Android
            // only a picture of the bar the glass grew from.
            pointerEvents={!up && resting ? 'box-none' : 'none'}
            accessibilityElementsHidden={up || !resting}
            importantForAccessibility={!up && resting ? 'auto' : 'no-hide-descendants'}
          >
            <ConnectedVoiceRow serverKey={call.serverKey} channelId={call.channelId} muted={call.muted} inGlass />
          </Animated.View>
          {/* The call, laid out at its open size so it doesn't reflow as the glass grows. */}
          <Animated.View
            style={[styles.call, { width: openWidth, height: openHeight }, callStyle]}
            pointerEvents={up ? 'box-none' : 'none'}
            accessibilityElementsHidden={!up}
            importantForAccessibility={up ? 'auto' : 'no-hide-descendants'}
          >
            <GestureDetector gesture={grabberDrag}>
              <View collapsable={false} style={styles.handle}>
                <View style={[styles.grabber, { backgroundColor: colors.textMuted }]} />
              </View>
            </GestureDetector>
            {inIt && <VoicePane serverKey={call.serverKey} channelId={call.channelId} asCall headerGesture={headerDrag} />}
          </Animated.View>
        </Animated.View>
      </Animated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  frame: { position: 'absolute' },
  clip: { overflow: 'hidden' },
  row: {
    position: 'absolute',
    top: 0,
    left: 0,
    paddingLeft: space.lg,
    paddingRight: space.xs,
    flexDirection: 'row',
    alignItems: 'center',
  },
  call: { position: 'absolute', top: 0, left: 0 },
  handle: { height: 18, alignItems: 'center', justifyContent: 'flex-end' },
  grabber: { width: GRABBER.width, height: GRABBER.height, borderRadius: GRABBER.height / 2, opacity: 0.6 },
});
