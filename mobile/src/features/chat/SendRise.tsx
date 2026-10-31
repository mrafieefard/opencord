// A message just sent from this device rises into the list from the
// composer, growing to its size as it settles, as Telegram's. Any other
// message (and this one once the server's copy replaces it) just appears.
import { type ReactNode, useEffect, useState } from 'react';
import { StyleSheet } from 'react-native';
import Animated, { interpolate, useAnimatedStyle, useReducedMotion, useSharedValue, withSpring } from 'react-native-reanimated';

import { forgetFreshSend, isFreshSend } from '@/state/freshSends';

/** From about where the composer's text was. */
const RISE = 56;
const FROM_SCALE = 0.9;

export interface SendRiseProps {
  nonce: string | undefined;
  children: ReactNode;
}

export function SendRise({ nonce, children }: SendRiseProps) {
  const reduceMotion = useReducedMotion();
  const [rising] = useState(() => !reduceMotion && isFreshSend(nonce));
  const progress = useSharedValue(rising ? 0 : 1);
  useEffect(() => {
    forgetFreshSend(nonce);
    if (rising) progress.set(withSpring(1, { damping: 18, stiffness: 240, mass: 0.8 }));
  }, [nonce, rising, progress]);
  const style = useAnimatedStyle(() => {
    const at = progress.get();
    return {
      opacity: interpolate(at, [0, 0.25], [0, 1], 'clamp'),
      transform: [{ translateY: (1 - at) * RISE }, { scale: FROM_SCALE + (1 - FROM_SCALE) * at }],
    };
  });
  return <Animated.View style={[styles.origin, style]}>{children}</Animated.View>;
}

const styles = StyleSheet.create({
  // It grows from the corner nearest the composer's send button.
  origin: { transformOrigin: 'bottom right' },
});
