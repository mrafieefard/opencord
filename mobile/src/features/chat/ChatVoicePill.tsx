// The voice connection over a chat, as the main page's above its tab bar:
// a floating pill above the composer. It steps aside while the keyboard is
// up, so the field keeps the room.
import { StyleSheet } from 'react-native';
import { useKeyboardState } from 'react-native-keyboard-controller';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';

import { ConnectedVoiceRow } from '@/features/voice/ConnectedBar';
import { useMyVoice } from '@/state/selectors';
import { GlassSurface, space } from '@/ui';

import { useChatDock } from './dock';

export interface ChatVoicePillProps {
  /** The server whose screen this is: its channels open in place. */
  serverKey: string;
  onOpenChannel: (channelId: string) => void;
}

const HEIGHT = 56;

export function ChatVoicePill({ serverKey, onOpenChannel }: ChatVoicePillProps) {
  const myVoice = useMyVoice();
  const keyboardUp = useKeyboardState((state) => state.isVisible);
  const { fieldGrowth } = useChatDock();
  // It rises with the field as the field grows over the messages.
  const rise = useAnimatedStyle(() => ({ transform: [{ translateY: -(fieldGrowth?.get() ?? 0) }] }));
  if (!myVoice || keyboardUp) return null;
  return (
    <Animated.View style={rise}>
    <GlassSurface interactive style={styles.pill} testID="chat-voice-pill">
      <ConnectedVoiceRow
        serverKey={myVoice.serverKey}
        channelId={myVoice.channelId}
        muted={myVoice.muted}
        onOpenHere={myVoice.serverKey === serverKey ? onOpenChannel : undefined}
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
