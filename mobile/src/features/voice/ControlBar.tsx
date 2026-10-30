// The voice call's controls (spec §8.6): 56 pt round buttons (Mute,
// Deafen, Soundboard, Leave). Music and, on iOS, the sound output have
// their buttons in the header. Toggles flip between surface/text (off)
// and accent/onAccent (on), and their glyph shows the state; Leave is
// recording with a white glyph.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon, onHue, space, useTheme } from '@/ui';

export interface ControlBarProps {
  muted: boolean;
  deafened: boolean;
  onToggleMute: () => void;
  onToggleDeafen: () => void;
  onOpenSoundboard?: () => void;
  onLeave: () => void;
}

const BUTTON_SIZE = 56;

export function ControlBar({
  muted,
  deafened,
  onToggleMute,
  onToggleDeafen,
  onOpenSoundboard,
  onLeave,
}: ControlBarProps) {
  const { colors } = useTheme();

  return (
    <View style={styles.bar}>
      {/* Mute */}
      <Touchable
        onPress={onToggleMute}
        accessible
        accessibilityRole="button"
        accessibilityLabel={muted ? 'Unmute' : 'Mute'}
        testID="control-mute"
      >
        <View
          style={[
            styles.button,
            { backgroundColor: muted ? colors.accent : colors.surface },
          ]}
        >
          <Icon
            name={muted ? 'micOff' : 'mic'}
            size={24}
            color={muted ? 'onAccent' : 'text'}
          />
        </View>
      </Touchable>

      {/* Deafen */}
      <Touchable
        onPress={onToggleDeafen}
        accessible
        accessibilityRole="button"
        accessibilityLabel={deafened ? 'Undeafen' : 'Deafen'}
        testID="control-deafen"
      >
        <View
          style={[
            styles.button,
            { backgroundColor: deafened ? colors.accent : colors.surface },
          ]}
        >
          <Icon
            name={deafened ? 'deafen' : 'listen'}
            size={24}
            color={deafened ? 'onAccent' : 'text'}
          />
        </View>
      </Touchable>

      {/* Soundboard */}
      <Touchable
        onPress={onOpenSoundboard}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Soundboard"
        testID="control-soundboard"
      >
        <View style={[styles.button, { backgroundColor: colors.surface }]}>
          <Icon name="soundboard" size={24} color="text" />
        </View>
      </Touchable>

      {/* Leave */}
      <Touchable
        onPress={onLeave}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Leave voice"
        testID="control-leave"
      >
        <View style={[styles.button, { backgroundColor: colors.recording }]}>
          <Icon name="leave" size={24} tint={onHue} />
        </View>
      </Touchable>
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    justifyContent: 'space-evenly',
    alignItems: 'center',
    paddingVertical: space.md,
  },
  button: {
    width: BUTTON_SIZE,
    height: BUTTON_SIZE,
    borderRadius: BUTTON_SIZE / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
