// The voice call's controls (spec §8.6): 56 pt round buttons (Mute,
// Deafen, Soundboard, Activities, the running activity, Leave), floating in
// one glass bar (iOS) or raised surface (Android) over the call. The
// running activity's button wears its icon and opens its page; with more
// than one running it offers them in a menu. Music and, on iOS, the sound output
// have their buttons in the header. Toggles flip between surface/text (off)
// and accent/onAccent (on), and their glyph shows the state; Leave is
// recording with a white glyph.
import { useCallback, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { PluginIcon } from '@/features/plugins/PluginIcon';
import { GlassSurface, Icon, Menu, onHue, space, useTheme, type MenuHandle } from '@/ui';

/** An activity running in the call, as its button shows it. */
export interface RunningActivity {
  sessionId: string;
  name: string;
  pluginId: string;
  icon: string;
}

export interface ControlBarProps {
  muted: boolean;
  deafened: boolean;
  onToggleMute: () => void;
  onToggleDeafen: () => void;
  onOpenSoundboard?: () => void;
  /** Given when this call can start an activity (plugins M4 Task 14). */
  onOpenActivities?: () => void;
  /** The activities running in the call, newest last; their button opens one's page. */
  running?: RunningActivity[];
  serverKey?: string;
  onOpenActivity?: (sessionId: string) => void;
  onLeave: () => void;
}

const BUTTON_SIZE = 56;

interface RunningActivityButtonProps {
  running: RunningActivity[];
  serverKey: string;
  onOpen: (sessionId: string) => void;
}

/** The running activity's button: its icon; with more than one, a menu of them. */
function RunningActivityButton({ running, serverKey, onOpen }: RunningActivityButtonProps) {
  const { colors } = useTheme();
  const menu = useRef<MenuHandle>(null);
  const newest = running[running.length - 1];
  const several = running.length > 1;
  const press = useCallback(() => {
    if (several) menu.current?.open();
    else if (newest) onOpen(newest.sessionId);
  }, [several, newest, onOpen]);
  if (!newest) return null;
  const button = (
    <Touchable
      onPress={press}
      accessible
      accessibilityRole="button"
      accessibilityLabel={several ? 'Open an activity' : `Open ${newest.name}`}
      testID="control-activity"
    >
      <View style={[styles.button, { backgroundColor: colors.surface }]}>
        <PluginIcon serverKey={serverKey} pluginId={newest.pluginId} icon={newest.icon} size={28} radius={7} />
      </View>
    </Touchable>
  );
  return several ? (
    <Menu ref={menu} actions={running.map((activity) => ({ id: activity.sessionId, title: activity.name }))} onAction={onOpen} trigger="press">
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
  running = [],
  serverKey,
  onOpenActivity,
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

      {/* Activities */}
      {onOpenActivities && (
        <Touchable
          onPress={onOpenActivities}
          accessible
          accessibilityRole="button"
          accessibilityLabel="Activities"
          testID="control-activities"
        >
          <View style={[styles.button, { backgroundColor: colors.surface }]}>
            <Icon name="extension" size={24} color="text" />
          </View>
        </Touchable>
      )}

      {/* The running activity: its page, or a choice of them */}
      {running.length > 0 && serverKey && onOpenActivity && (
        <RunningActivityButton running={running} serverKey={serverKey} onOpen={onOpenActivity} />
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
