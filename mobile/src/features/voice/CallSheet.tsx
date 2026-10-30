// The call sheet (callPosition.ts): the voice bar's own glass grown into the
// call, as Apple Maps' card grows. Its frame blends from the bar's (where it
// was last touched) to the full screen with one shared position: a bar
// pulled up stretches under the finger, and here, dragging the grabber or
// the header down shrinks it back the same way; letting go springs it open
// or back into the bar. The bar's row fades out as the call fades in. The
// dimmed screen above, the chevron, Android's back, or leaving the call put
// it away. The music player and the soundboard open over it as sheets.
import { useEffect, useMemo, useState } from 'react';
import { BackHandler, Pressable, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Animated, { interpolate, useAnimatedReaction, useAnimatedStyle, withSpring } from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

import { useMyVoice } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { GlassSurface, space, useTheme } from '@/ui';

import {
  BAR_HEIGHT,
  CALL_FLICK,
  CALL_SETTLE,
  CALL_SPRING,
  callOrigin,
  callProgress,
  callTravel,
  closeCall,
  useCallTop,
  useFallbackOrigin,
} from './callPosition';
import { ConnectedVoiceRow } from './ConnectedBar';
import { VoicePane } from './VoicePane';

/** The bar's pill and the open call's corners, as round as the screen's own. */
const BAR_CORNER = BAR_HEIGHT / 2;
const OPEN_CORNER = 38;
const SCRIM = 0.35;
const GRABBER = { width: 36, height: 5 };

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
  const call = useAppStore((state) => state.call);
  const myVoice = useMyVoice();
  const inIt = call !== null && myVoice?.serverKey === call.serverKey && myVoice.channelId === call.channelId;
  // Takes touches only while it is out of the bar, however little.
  const [up, setUp] = useState(false);
  useAnimatedReaction(
    () => callProgress.get() > 0.01,
    (now, before) => {
      if (now !== before) scheduleOnRN(setUp, now);
    },
  );

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

  // The glass: from the bar's frame to the open call's, corners and all.
  const glassStyle = useAnimatedStyle(() => {
    const at = callProgress.get();
    const touched = callOrigin.get();
    const from = touched.width > 0 ? touched : fallback;
    return {
      opacity: at > 0.001 ? 1 : 0,
      top: from.y + (top - from.y) * at,
      left: from.x * (1 - at),
      width: from.width + (screenWidth - from.width) * at,
      height: from.height + (openHeight - from.height) * at,
      borderRadius: BAR_CORNER + (OPEN_CORNER - BAR_CORNER) * at,
    };
  });
  const scrimStyle = useAnimatedStyle(() => ({ opacity: callProgress.get() * SCRIM }));
  // The bar's row, as it was, fading as the glass leaves it behind.
  const rowStyle = useAnimatedStyle(() => {
    const touched = callOrigin.get();
    return {
      width: (touched.width > 0 ? touched : fallback).width,
      opacity: interpolate(callProgress.get(), [0, 0.2], [1, 0], 'clamp'),
    };
  });
  const callStyle = useAnimatedStyle(() => ({ opacity: interpolate(callProgress.get(), [0.15, 0.55], [0, 1], 'clamp') }));

  if (!call) return null;
  return (
    <View style={StyleSheet.absoluteFill} pointerEvents={up ? 'box-none' : 'none'}>
      <Animated.View style={[StyleSheet.absoluteFill, { backgroundColor: colors.scrim }, scrimStyle]}>
        <Pressable style={StyleSheet.absoluteFill} onPress={closeCall} accessibilityLabel="Close the call" />
      </Animated.View>
      <Animated.View
        style={[styles.glass, glassStyle]}
        accessibilityViewIsModal={up}
        // Out of sight, out of VoiceOver's reach too.
        accessibilityElementsHidden={!up}
        importantForAccessibility={up ? 'auto' : 'no-hide-descendants'}
        testID="call-sheet"
      >
        <GlassSurface style={StyleSheet.absoluteFill} />
        <Animated.View
          style={[styles.row, rowStyle]}
          pointerEvents="none"
          // Only a picture of the bar the glass grew from.
          accessibilityElementsHidden
          importantForAccessibility="no-hide-descendants"
        >
          {inIt && <ConnectedVoiceRow serverKey={call.serverKey} channelId={call.channelId} muted={myVoice.muted} />}
        </Animated.View>
        {/* The call, laid out at its open size so it doesn't reflow as the glass grows. */}
        <Animated.View style={[styles.call, { width: screenWidth, height: openHeight }, callStyle]}>
          <GestureDetector gesture={grabberDrag}>
            <View collapsable={false} style={styles.handle}>
              <View style={[styles.grabber, { backgroundColor: colors.textMuted }]} />
            </View>
          </GestureDetector>
          {inIt && <VoicePane serverKey={call.serverKey} channelId={call.channelId} asCall headerGesture={headerDrag} />}
        </Animated.View>
      </Animated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  glass: { position: 'absolute', overflow: 'hidden' },
  row: {
    position: 'absolute',
    top: 0,
    left: 0,
    height: BAR_HEIGHT,
    paddingLeft: space.lg,
    paddingRight: space.xs,
    flexDirection: 'row',
    alignItems: 'center',
  },
  call: { position: 'absolute', top: 0, left: 0 },
  handle: { height: 18, alignItems: 'center', justifyContent: 'flex-end' },
  grabber: { width: GRABBER.width, height: GRABBER.height, borderRadius: GRABBER.height / 2, opacity: 0.6 },
});
