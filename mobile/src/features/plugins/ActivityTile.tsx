// An activity in the call's grid (plugins M4 plan, Task 15), after every
// person: its icon, name, who started it and how many have it open. As the
// tile gets short the icon goes first, then the viewers line, then the
// starter; the text never shrinks. Tapping opens it; a long press offers
// the activity's menu.
import type { ActivitySessionRecord } from '@opencord/core-native';
import { StyleSheet, View } from 'react-native';

import { openActivity } from '@/state/actions/plugins';
import { memberName, useServer } from '@/state/selectors';
import { Menu, Row, Text, radius, space, useTheme } from '@/ui';

import { activityActions, runActivityAction } from './activityActions';
import { PluginIcon } from './PluginIcon';

export interface ActivityTileProps {
  serverKey: string;
  session: ActivitySessionRecord;
  width: number;
  height: number;
}

/** Below these heights a line goes: the icon first, then viewers, then the starter. */
const WITH_ICON = 132;
const WITH_VIEWERS = 96;
const WITH_STARTER = 72;

export function viewersLine(count: number): string {
  if (count === 0) return 'No one has it open';
  if (count === 1) return '1 person has it open';
  return `${count} people have it open`;
}

export function ActivityTile({ serverKey, session, width, height }: ActivityTileProps) {
  const { colors } = useTheme();
  const server = useServer(serverKey);
  const starter = memberName(server?.snapshot ?? null, session.starterId) ?? 'Someone';
  return (
    <Menu
      actions={activityActions(server, session, false)}
      onAction={(id) => runActivityAction(serverKey, session, id)}
      trigger="longPress"
      testID={`activity-tile-${session.id}`}
    >
      <View style={[styles.tile, { width, height, backgroundColor: colors.surface }]}>
        <Row height={height} accessibilityLabel={session.name} onPress={() => openActivity(serverKey, session.channelId, session.id)} style={styles.row}>
          <View style={styles.body}>
            {height >= WITH_ICON && <PluginIcon serverKey={serverKey} pluginId={session.pluginId} icon={session.icon} size={40} radius={10} />}
            <Text variant="headline" numberOfLines={1}>
              {session.name}
            </Text>
            {height >= WITH_STARTER && (
              <Text variant="caption" color="textSecondary" numberOfLines={1}>
                {`Started by ${starter}`}
              </Text>
            )}
            {height >= WITH_VIEWERS && (
              <Text variant="caption" color="textMuted" numberOfLines={1}>
                {viewersLine(session.viewerIds.length)}
              </Text>
            )}
          </View>
        </Row>
      </View>
    </Menu>
  );
}

const styles = StyleSheet.create({
  tile: { borderRadius: radius.tile, overflow: 'hidden' },
  row: { paddingHorizontal: space.md },
  body: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: space.xs },
});
