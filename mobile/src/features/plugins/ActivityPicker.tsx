// Starting an activity in the call (plugins M4 plan, Task 14; §15.2): the
// app's sheet with one row per registered activity, by plugin name then
// name. A row that runs already says Running; while the call holds the
// most it can, the others are disabled. Pressing a row starts it (or
// finds it running) and opens it.
import * as PluginArea from '@opencord/plugin-area';
import { router } from 'expo-router';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';

import { openCall } from '@/features/voice/callPosition';
import { ACTIVITY_SESSIONS_PER_CALL, openActivity, startActivity } from '@/state/actions/plugins';
import { activitySessions, canStartActivities, pluginActivities, useServer } from '@/state/selectors';
import type { ServerState } from '@/state/types';
import { Row, SheetHeader, Text, space, useSheetBottomInset } from '@/ui';

import { PluginIcon } from './PluginIcon';

const ICON_SIZE = 40;
const ICON_RADIUS = 10;
const DISABLED_OPACITY = 0.4;

/** The control bar's Activities button: an activity, the permission and the renderer. */
export function showActivitiesButton(server: ServerState | undefined, channelId: string, available = PluginArea.available): boolean {
  return available && pluginActivities(server).length > 0 && canStartActivities(server, channelId);
}

export interface ActivityPickerProps {
  serverKey: string;
  channelId: string;
  onClose?: () => void;
}

export function ActivityPicker({ serverKey, channelId, onClose }: ActivityPickerProps) {
  const server = useServer(serverKey);
  const bottomInset = useSheetBottomInset();
  const close = onClose ?? (() => router.back());
  const activities = useMemo(
    () =>
      [...pluginActivities(server)].sort(
        (a, b) => a.pluginName.localeCompare(b.pluginName) || a.name.localeCompare(b.name),
      ),
    [server],
  );
  const sessions = activitySessions(server, channelId);
  const full = sessions.length >= ACTIVITY_SESSIONS_PER_CALL;

  // One start at a time: a second tap mustn't close the sheet twice
  // (the second `router.back()` would leave the server screen too).
  const starting = useRef(false);
  const [pending, setPending] = useState<string | null>(null);
  // Swiped away or backed out of while the start was on its way: its
  // `router.back()` is done already.
  const mounted = useRef(true);
  useEffect(
    () => () => {
      mounted.current = false;
    },
    [],
  );
  const start = async (pluginId: string, activityKey: string) => {
    if (starting.current) return;
    starting.current = true;
    setPending(`${pluginId}/${activityKey}`);
    const session = await startActivity(serverKey, channelId, pluginId, activityKey);
    // Refused: another try may go. Started: the sheet is on its way out.
    if (!session) {
      starting.current = false;
      if (mounted.current) setPending(null);
      return;
    }
    if (mounted.current) close();
    openActivity(serverKey, channelId, session.id);
    // The call shows it; Android put the call away to open this sheet.
    openCall(serverKey, channelId);
  };

  return (
    <View style={styles.root}>
      <SheetHeader title="Activities" onClose={close} />
      <ScrollView contentContainerStyle={{ paddingBottom: bottomInset + space.md }}>
        {activities.map((activity) => {
          const running = sessions.some((s) => s.pluginId === activity.pluginId && s.activityKey === activity.activityKey);
          const disabled = full && !running;
          return (
            <View key={`${activity.pluginId}/${activity.activityKey}`} testID={`activity-row-${activity.pluginId}/${activity.activityKey}`} style={disabled && { opacity: DISABLED_OPACITY }}>
              <Row
                height={64}
                accessibilityLabel={activity.name}
                onPress={disabled ? undefined : () => void start(activity.pluginId, activity.activityKey)}
                style={styles.row}
              >
                <View style={styles.icon}>
                  <PluginIcon serverKey={serverKey} pluginId={activity.pluginId} icon={activity.icon} size={ICON_SIZE} radius={ICON_RADIUS} />
                </View>
                <View style={styles.text}>
                  <Text variant="body" numberOfLines={1}>
                    {activity.name}
                  </Text>
                  <Text variant="footnote" color="textSecondary" numberOfLines={1}>
                    {activity.description !== '' ? activity.description : activity.pluginName}
                  </Text>
                </View>
                {pending === `${activity.pluginId}/${activity.activityKey}` ? (
                  <Text variant="footnote" color="textSecondary">
                    Starting…
                  </Text>
                ) : (
                  running && (
                    <Text variant="footnote" color="textSecondary">
                      Running
                    </Text>
                  )
                )}
              </Row>
            </View>
          );
        })}
        {full && (
          <Text variant="footnote" color="textSecondary" style={styles.full}>
            {`This call already has ${ACTIVITY_SESSIONS_PER_CALL} activities.`}
          </Text>
        )}
      </ScrollView>
    </View>
  );
}

// The sheet's side margin, as the soundboard's.
const GUTTER = space.lg;

const styles = StyleSheet.create({
  root: { flex: 1, paddingHorizontal: GUTTER },
  row: { borderRadius: 12 },
  icon: { marginRight: space.md },
  text: { flex: 1, gap: 2, marginRight: space.sm },
  full: { paddingTop: space.md },
});
