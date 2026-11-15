// The voice call's controls (spec §8.6): 56 pt round buttons (Mute,
// Deafen, Soundboard, Activities, Leave), floating in one glass bar (iOS)
// or raised surface (Android) over the call. Activities is on, as Mute is,
// while an activity runs in the call: then it offers a menu (open it,
// change it, close it); off, it opens the picker. Music and, on iOS, the sound output
// have their buttons in the header. Toggles flip between surface/text (off)
// and accent/onAccent (on), and their glyph shows the state; Leave is
// recording with a white glyph.
import { useCallback, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { GlassSurface, Icon, Menu, onHue, space, useTheme, type MenuAction, type MenuHandle } from '@/ui';

export interface ControlBarProps {
  muted: boolean;
  deafened: boolean;
  onToggleMute: () => void;
  onToggleDeafen: () => void;
  onOpenSoundboard?: () => void;
  /** Given when this call can start an activity (plugins M4 Task 14): the picker. */
  onOpenActivities?: () => void;
  /** An activity runs in the call: the button is on, and offers `activityMenu`. */
  activityActive?: boolean;
  activityMenu?: MenuAction[];
  onActivityAction?: (id: string) => void;
  onLeave: () => void;
}

const BUTTON_SIZE = 56;

interface ActivitiesButtonProps {
  active: boolean;
  menu: MenuAction[];
  onAction?: (id: string) => void;
  onOpenPicker?: () => void;
}

/** Activities: off, the picker; on (one running), its menu. */
function ActivitiesButton({ active, menu: actions, onAction, onOpenPicker }: ActivitiesButtonProps) {
  const { colors } = useTheme();
  const menu = useRef<MenuHandle>(null);
  const withMenu = active && actions.length > 0 && onAction !== undefined;
  const press = useCallback(() => {
    if (withMenu) menu.current?.open();
    else onOpenPicker?.();
  }, [withMenu, onOpenPicker]);
  const button = (
    <Touchable
      onPress={press}
      accessible
      accessibilityRole="button"
      accessibilityLabel="Activities"
      accessibilityState={{ selected: active }}
      testID="control-activities"
    >
      <View style={[styles.button, { backgroundColor: active ? colors.accent : colors.surface }]}>
        <Icon name="extension" size={24} color={active ? 'onAccent' : 'text'} />
      </View>
    </Touchable>
  );
  return withMenu ? (
    <Menu ref={menu} actions={actions} onAction={onAction} trigger="press" testID="activities-menu">
      {button}
    </Menu>
  ) : (
    button
  );
}

export function ControlBar({
  muted,
  deafened,
  onToggleMute,
  onToggleDeafen,
  onOpenSoundboard,
  onOpenActivities,
  activityActive = false,
  activityMenu = [],
  onActivityAction,
  onLeave,
}: ControlBarProps) {
  const { colors } = useTheme();

  return (
    <GlassSurface style={styles.bar} testID="control-bar">
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

      {/* Activities: on while one runs */}
      {(onOpenActivities || activityActive) && (
        <ActivitiesButton active={activityActive} menu={activityMenu} onAction={onActivityAction} onOpenPicker={onOpenActivities} />
      )}

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
    </GlassSurface>
  );
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    justifyContent: 'space-evenly',
    alignItems: 'center',
    paddingVertical: space.sm,
    marginHorizontal: space.md,
    marginBottom: space.sm,
    borderRadius: BUTTON_SIZE / 2 + space.sm,
  },
  button: {
    width: BUTTON_SIZE,
    height: BUTTON_SIZE,
    borderRadius: BUTTON_SIZE / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
