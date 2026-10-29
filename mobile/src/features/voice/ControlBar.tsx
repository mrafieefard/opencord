// The voice call's controls (spec §8.6): four 56 pt round buttons
// (Mute, Deafen, Music, Leave). Toggles flip between surface/text (off)
// and accent/onAccent (on); Leave is recording.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon, space, useTheme } from '@/ui';

export interface ControlBarProps {
  muted: boolean;
  deafened: boolean;
  onToggleMute: () => void;
  onToggleDeafen: () => void;
  onOpenMusic?: () => void;
  onLeave: () => void;
}

const BUTTON_SIZE = 56;

export function ControlBar({
  muted,
  deafened,
  onToggleMute,
  onToggleDeafen,
  onOpenMusic,
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
            name="deafen"
            size={24}
            color={deafened ? 'onAccent' : 'text'}
          />
        </View>
      </Touchable>

      {/* Music */}
      <Touchable
        onPress={onOpenMusic}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Music"
        testID="control-music"
      >
        <View style={[styles.button, { backgroundColor: colors.surface }]}>
          <Icon name="music" size={24} color="text" />
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
          <Icon name="leave" size={24} color="onAccent" />
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
