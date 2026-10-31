// The voice connection over a chat on iOS: the room for the call's floating
// glass pill (CallSheet), above the composer as the main page's is above
// its tab bar. The pill itself is the call's own glass, which moves here
// from above the tab bar and grows into the call when pulled; this keeps
// its place in the dock and says where that is. It gives the place up while
// the keyboard is up or the sidebar is over the chat, and the pill moves on.
import { type ComponentRef, useCallback, useEffect, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { useKeyboardState } from 'react-native-keyboard-controller';
import { useAnimatedReaction } from 'react-native-reanimated';

import { BAR_HEIGHT, slotRise } from '@/features/voice/callPosition';
import { setVoiceSlot } from '@/state/actions/ui';
import { useMyVoice } from '@/state/selectors';
import { space } from '@/ui';

import { AWAY_DROP, useChatDock } from './dock';

/** The dock moves a little while it settles (the keyboard going, the sidebar leaving): measured again once it has. */
const SETTLED_MS = 300;

export interface ChatVoicePillProps {
  /** The sidebar is over the chat. */
  covered?: boolean;
}

export function ChatVoicePill({ covered = false }: ChatVoicePillProps) {
  const myVoice = useMyVoice();
  const keyboardUp = useKeyboardState((state) => state.isVisible);
  const { fieldGrowth, away } = useChatDock();
  const slotRef = useRef<ComponentRef<typeof View>>(null);
  const holds = myVoice !== null && !keyboardUp && !covered;

  // Where the room is once the dock is in place: coming back from the
  // sidebar, the dock is still lower by what is left of its step aside.
  const measure = useCallback(() => {
    slotRef.current?.measureInWindow((_x, y, _width, height) => {
      if (height > 0) setVoiceSlot({ y: Math.round(y - (away?.get() ?? 0) * AWAY_DROP), height });
    });
  }, [away]);
  useEffect(() => {
    if (!holds) return undefined;
    measure();
    const settled = setTimeout(measure, SETTLED_MS);
    return () => {
      clearTimeout(settled);
      setVoiceSlot(null);
    };
  }, [holds, measure]);

  // The pill rises with the field as the field grows over the messages.
  useAnimatedReaction(
    () => fieldGrowth?.get() ?? 0,
    (now) => slotRise.set(now),
  );
  useEffect(() => () => slotRise.set(0), []);

  if (!myVoice || keyboardUp) return null;
  return (
    <View
      ref={slotRef}
      collapsable={false}
      pointerEvents="none"
      style={styles.slot}
      onLayout={holds ? measure : undefined}
      testID="chat-voice-pill"
    />
  );
}

const styles = StyleSheet.create({
  slot: { height: BAR_HEIGHT, marginHorizontal: space.sm, marginBottom: space.sm },
});
