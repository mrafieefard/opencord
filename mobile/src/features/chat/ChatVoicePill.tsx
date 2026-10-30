// The voice connection over a chat, as the main page's above its tab bar:
// a floating pill above the composer, opening the call by a tap or a pull
// up. It steps aside while the keyboard is up, so the field keeps the room.
import { StyleSheet } from 'react-native';
import { useKeyboardState } from 'react-native-keyboard-controller';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';

import { ConnectedVoiceRow } from '@/features/voice/ConnectedBar';
import { useMyVoice } from '@/state/selectors';
import { GlassSurface, space } from '@/ui';

import { callProgress } from '@/features/voice/callPosition';

import { useChatDock } from './dock';

const HEIGHT = 56;

export function ChatVoicePill() {
  const myVoice = useMyVoice();
  const keyboardUp = useKeyboardState((state) => state.isVisible);
  const { fieldGrowth } = useChatDock();
  // It rises with the field as the field grows over the messages.
  // The call's glass grows out of it: once it does, the glass is the pill.
  const rise = useAnimatedStyle(() => ({
    opacity: callProgress.get() > 0.001 ? 0 : 1,
    transform: [{ translateY: -(fieldGrowth?.get() ?? 0) }],
  }));
  if (!myVoice || keyboardUp) return null;
  return (
    <Animated.View style={rise}>
    <GlassSurface interactive style={styles.pill} testID="chat-voice-pill">
      <ConnectedVoiceRow
        serverKey={myVoice.serverKey}
        channelId={myVoice.channelId}
        muted={myVoice.muted}
      />
    </GlassSurface>
    </Animated.View>
  );
}

const styles = StyleSheet.create({
  pill: {
    height: HEIGHT,
    borderRadius: HEIGHT / 2,
    marginHorizontal: space.sm,
    marginBottom: space.sm,
    paddingLeft: space.lg,
    paddingRight: space.xs,
    flexDirection: 'row',
    alignItems: 'center',
  },
});
