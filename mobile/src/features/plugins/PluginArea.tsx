// A plugin's screen (plugins M4 plan, Task 11): the native view over the
// session's area while the plugin runs, and React Native's own words in its
// place otherwise. It tells the plugin when it's paused, hands the system
// back to it first, and keeps its setup (theme, keyboard) current. The area
// belongs to the session: unmounting this keeps it, closing the session
// destroys it.
import { PluginAreaView } from '@opencord/plugin-area';
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ComponentRef } from 'react';
import { AccessibilityInfo, ActivityIndicator, AppState, BackHandler, ScrollView, StyleSheet, View, useWindowDimensions } from 'react-native';
import { useKeyboardState } from 'react-native-keyboard-controller';

import { Button, Text, fonts, space, useTheme } from '@/ui';

import { areaSetup } from './areaSetup';
import { keyboardOverlap } from './keyboardOverlap';
import type { AreaStatus, ViewSession } from './viewSession';

export interface PluginAreaProps {
  session: ViewSession;
  pluginId: string;
  /** Out of sight (another pane, a minimised activity): the plugin hears `pause`. */
  paused: boolean;
  /** Back with nothing left for the plugin to close. */
  onLeave: () => void;
  /** The error and closed states' Reload: a new session in place of this one. */
  onReload: () => void;
  /** Whether the iOS swipe-back may leave: not while the plugin uses back. */
  onSwipeBackChange?: (enabled: boolean) => void;
}

/** The closed op's reasons that mean the channel itself is gone or stopped. */
const UNAVAILABLE = new Set(['plugin_stopped', 'lost_access', 'channel_deleted', 'unavailable']);

function useStatus(session: ViewSession): AreaStatus {
  const subscribe = useCallback(
    (changed: () => void) => {
      const stop = session.subscribe(changed);
      return () => {
        stop();
      };
    },
    [session],
  );
  return useSyncExternalStore(subscribe, () => session.status);
}

function useInBackground(): boolean {
  const [background, setBackground] = useState(AppState.currentState === 'background');
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => setBackground(state === 'background'));
    return () => subscription.remove();
  }, []);
  return background;
}

function useReduceMotion(): boolean {
  const [reduce, setReduce] = useState(false);
  useEffect(() => {
    let live = true;
    AccessibilityInfo.isReduceMotionEnabled().then(
      (value) => live && setReduce(value),
      () => {},
    );
    const subscription = AccessibilityInfo.addEventListener('reduceMotionChanged', setReduce);
    return () => {
      live = false;
      subscription.remove();
    };
  }, []);
  return reduce;
}

export function PluginArea({ session, pluginId, paused, onLeave, onReload, onSwipeBackChange }: PluginAreaProps) {
  const { colors, scheme } = useTheme();
  const status = useStatus(session);
  const background = useInBackground();
  const reduceMotion = useReduceMotion();
  const { fontScale, height: screenHeight } = useWindowDimensions();
  const serverKey = session.place.serverKey;

  // The keyboard covers the area's bottom: pad it by that, measured on
  // screen (keyboardOverlap.ts).
  const keyboard = useKeyboardState((state) => (state.isVisible ? state.height : 0));
  const frame = useRef<ComponentRef<typeof View>>(null);
  const [areaBottom, setAreaBottom] = useState<number | null>(null);
  const measure = useCallback(() => {
    frame.current?.measureInWindow((_x, y, _width, height) => setAreaBottom(y + height));
  }, []);
  useEffect(measure, [measure, keyboard]);
  const lift = areaBottom === null ? 0 : keyboardOverlap({ areaBottom, screenHeight, keyboardHeight: keyboard });

  // The setup follows the theme and the keyboard; the area redraws in place.
  const setupJson = useMemo(
    () =>
      areaSetup({
        colors,
        scheme,
        // The keyboard shrinks the area instead (padded below):
        // the renderer's sheets sit on the area's bottom, not on an inset.
        insets: { top: 0, right: 0, bottom: 0, left: 0 },
        textScale: fontScale,
        reduceMotion,
        serverKey,
        pluginId,
      }),
    [colors, scheme, fontScale, reduceMotion, serverKey, pluginId],
  );
  useEffect(() => session.setSetup(setupJson), [session, setupJson]);

  // `pause` and `resume`, once per change (the desktop's _tellPaused).
  const pausedNow = paused || background;
  const told = useRef(false);
  useEffect(() => {
    if (told.current === pausedNow) return;
    told.current = pausedNow;
    session.send({ kind: 'lifecycle', event: pausedNow ? 'pause' : 'resume' });
  }, [session, pausedNow]);

  // Android's back: the area and the plugin first.
  const leave = useRef(onLeave);
  useEffect(() => {
    leave.current = onLeave;
  }, [onLeave]);
  useEffect(() => {
    if (paused) return undefined;
    const subscription = BackHandler.addEventListener('hardwareBackPress', () => {
      session.back().then(
        (took) => {
          if (!took) leave.current();
        },
        () => leave.current(),
      );
      return true;
    });
    return () => subscription.remove();
  }, [session, paused]);

  const swipeBack = status.state !== 'running' || (status.overlays === 0 && status.back !== true);
  useEffect(() => onSwipeBackChange?.(swipeBack), [onSwipeBackChange, swipeBack]);

  // The native view is there from the start: the engine needs a view to lay
  // out and send the context that opens the plugin's view. Until it runs,
  // React Native's own words cover it.
  return (
    <View style={styles.fill}>
      <View ref={frame} onLayout={measure} style={[styles.fill, { paddingBottom: lift }]} testID="plugin-area-frame">
        <PluginAreaView areaId={session.areaId} style={styles.fill} />
      </View>
      {status.state !== 'running' && !(status.state === 'closed' && status.reason === 'left_call') && (
        <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.chat }]} testID="plugin-area-cover">
          <Cover status={status} onReload={onReload} />
        </View>
      )}
    </View>
  );
}

/** What React Native draws over the area while the plugin isn't running. */
function Cover({ status, onReload }: { status: AreaStatus; onReload: () => void }) {
  const { colors } = useTheme();
  if (status.state === 'loading') {
    return (
      <View style={styles.center} testID="plugin-area-loading">
        <ActivityIndicator color={colors.textMuted} />
      </View>
    );
  }
  if (status.state === 'error') {
    return (
      <View style={styles.center}>
        <Text variant="headline">This plugin hit an error</Text>
        <Button title="Reload" variant="secondary" onPress={onReload} />
        {status.detail !== '' && (
          <ScrollView style={styles.detail} contentContainerStyle={styles.detailBody}>
            <Text variant="footnote" color="textSecondary" selectable style={{ fontFamily: fonts.mono }}>
              {status.detail}
            </Text>
          </ScrollView>
        )}
      </View>
    );
  }
  if (status.state === 'too_old') {
    return (
      <View style={styles.center}>
        <Text variant="headline">Update Opencord to open this</Text>
      </View>
    );
  }
  if (UNAVAILABLE.has(status.reason)) {
    return (
      <View style={styles.center}>
        <Text variant="headline">This channel is unavailable</Text>
      </View>
    );
  }
  if (status.reason === 'activity_ended') {
    return (
      <View style={styles.center}>
        <Text variant="headline">This activity ended</Text>
      </View>
    );
  }
  return (
    <View style={styles.center}>
      <Text variant="headline">This plugin closed this screen</Text>
      <Button title="Reload" variant="secondary" onPress={onReload} />
    </View>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: space.md, padding: space.lg },
  detail: { alignSelf: 'stretch', maxHeight: 240 },
  detailBody: { padding: space.sm },
});
