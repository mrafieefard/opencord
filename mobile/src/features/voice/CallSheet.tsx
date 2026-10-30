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
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import { useKeyboardState } from 'react-native-keyboard-controller';
import Animated, {
  interpolate,
  useAnimatedReaction,
  useAnimatedStyle,
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
  CALL_FLICK,
  CALL_SETTLE,
  CALL_SPRING,
  type CallFrame,
  PILL_GAP,
  callOrigin,
  callProgress,
  callTravel,
  closeCall,
  slotRise,
  useCallTop,
  useFallbackOrigin,
} from './callPosition';
import { ConnectedVoiceRow } from './ConnectedBar';
import { VoicePane } from './VoicePane';

/** The open call's corners, as round as the screen's own. */
const OPEN_CORNER = 38;
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

/**
 * Where the floating pill rests (iOS), if anywhere now: in a chat's room
 * for it, else above the tab bar while that shows. Not while typing, nor
 * over the call's own page.
 */
function useRest(screenWidth: number, screenHeight: number): CallFrame | null {
  const call = useMyVoice();
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
    if (place === 'chat') return { x: CHAT_SIDE, y: slotY, width: screenWidth - CHAT_SIDE * 2, height: slotHeight };
    if (place === 'tabs') {
      return {
        x: TAB_SIDE,
        y: screenHeight - tabInset - PILL_GAP - BAR_HEIGHT,
        width: screenWidth - TAB_SIDE * 2,
        height: BAR_HEIGHT,
      };
    }
    return null;
  }, [place, slotY, slotHeight, screenWidth, screenHeight, tabInset]);
}

/** A downward drag on the call's top: it shrinks back towards the bar with the finger. */
function dragDown(top: number, screen: number) {
  return Gesture.Pan()
    .activeOffsetY(8)
    .failOffsetX([-16, 16])
    .onUpdate((event) => {
      const travel = callTravel(callOrigin.get(), top, screen);
      callProgress.set(Math.min(Math.max(1 - event.translationY / travel, 0), 1));
    })
    .onEnd((event) => {
      const travel = callTravel(callOrigin.get(), top, screen);
      const away = event.translationY > travel * CALL_SETTLE || event.velocityY > CALL_FLICK;
      callProgress.set(withSpring(away ? 0 : 1, { ...CALL_SPRING, velocity: -event.velocityY / travel }));
    });
}

export function CallSheet() {
  const { colors } = useTheme();
  const { width: screenWidth, height: screenHeight } = useWindowDimensions();
  const top = useCallTop();
  const fallback = useFallbackOrigin();
  // The call is this device's: there is one at most. Mounted (unseen) for
  // as long as it lasts, so the glass is there the moment a bar is pulled.
  const call = useMyVoice();
  const inIt = call !== null;
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
  const rest = useRest(screenWidth, screenHeight);
  const moveFrom = useSharedValue<CallFrame>(rest ?? NO_FRAME);
  const moveTo = useSharedValue<CallFrame>(rest ?? NO_FRAME);
  const move = useSharedValue(1);
  const shown = useSharedValue(rest ? 1 : 0);
  const wasShown = useRef(rest !== null);
  // Reanimated can lose the last frames of a layout animation that no
  // React render follows, leaving the pill short of its place: once the
  // move settles, a render commits where it is.
  const [, setSettled] = useState(0);
  const settle = useCallback(() => setSettled((count) => count + 1), []);
  useEffect(() => {
    if (!rest) {
      wasShown.current = false;
      shown.set(withTiming(0, { duration: FADE_MS }));
      return;
    }
    // A drag measures its travel from here.
    callOrigin.set(rest);
    if (wasShown.current) {
      // From wherever it is now, mid-move or not.
      moveFrom.set(between(moveFrom.get(), moveTo.get(), move.get()));
      moveTo.set(rest);
      move.set(0);
      move.set(
        withSpring(1, MOVE_SPRING, (done) => {
          if (done) scheduleOnRN(settle);
        }),
      );
    } else {
      moveFrom.set(rest);
      moveTo.set(rest);
      move.set(1);
    }
    wasShown.current = true;
    shown.set(withTiming(1, { duration: FADE_MS }));
  }, [rest, moveFrom, moveTo, move, shown, settle]);
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

  const grabberDrag = useMemo(() => dragDown(top, screenHeight), [top, screenHeight]);
  const headerDrag = useMemo(() => dragDown(top, screenHeight), [top, screenHeight]);
  const openHeight = screenHeight - top;

  // Where the glass grows from: the floating pill's place (risen with a
  // chat's field), or the bar last touched.
  const start = (): CallFrame => {
    'worklet';
    if (chrome.floatingVoice) {
      const at = between(moveFrom.get(), moveTo.get(), move.get());
      return { ...at, y: at.y - slotRise.get() };
    }
    const touched = callOrigin.get();
    return touched.width > 0 ? touched : fallback;
  };

  // The glass's frame: from the bar's to the open call's.
  const frameStyle = useAnimatedStyle(() => {
    const at = callProgress.get();
    const from = start();
    return {
      opacity: Math.max(shown.get(), at > 0.001 ? 1 : 0),
      top: from.y + (top - from.y) * at,
      left: from.x * (1 - at),
      width: from.width + (screenWidth - from.width) * at,
      height: from.height + (openHeight - from.height) * at,
    };
  });
  // Its corners: from the bar's capsule to the screen's own, on the glass
  // and on what it holds alike.
  const cornerStyle = useAnimatedStyle(() => {
    const from = start();
    return { borderRadius: from.height / 2 + (OPEN_CORNER - from.height / 2) * callProgress.get() };
  });
  const fillStyle = useAnimatedStyle(() => ({
    opacity: interpolate(callProgress.get(), [FILL_FROM, FILL_TO], [0, 1], 'clamp'),
  }));
  const scrimStyle = useAnimatedStyle(() => ({ opacity: callProgress.get() * SCRIM }));
  // The bar's row, as it was, fading as the glass leaves it behind.
  const rowStyle = useAnimatedStyle(() => {
    const from = start();
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
            {inIt && <ConnectedVoiceRow serverKey={call.serverKey} channelId={call.channelId} muted={call.muted} inGlass />}
          </Animated.View>
          {/* The call, laid out at its open size so it doesn't reflow as the glass grows. */}
          <Animated.View
            style={[styles.call, { width: screenWidth, height: openHeight }, callStyle]}
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
