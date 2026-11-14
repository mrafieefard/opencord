// The opened activity (plugins M4 plan, Task 15), in place of the call's
// grid: the people in one strip of small tiles, a bar with the activity's
// icon, name, menu and Close, then the plugin's screen.
import type { ActivitySessionRecord } from '@opencord/core-native';
import { useRef } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';

import { ParticipantTile } from '@/features/voice/ParticipantTile';
import { openActivity } from '@/state/actions/plugins';
import { useServer, useVoiceMembers } from '@/state/selectors';
import { IconButton, Menu, Text, space, useTheme, type MenuHandle } from '@/ui';

import { activityActions, runActivityAction } from './activityActions';
import { closeOpenActivity } from './activityHolder';
import { PluginArea } from './PluginArea';
import { PluginIcon } from './PluginIcon';
import type { ViewSession } from './viewSession';

const STRIP_HEIGHT = 56;
const STRIP_TILE = 72;

export interface ActivityStageProps {
  serverKey: string;
  channelId: string;
  record: ActivitySessionRecord;
  session: ViewSession;
}

export function ActivityStage({ serverKey, channelId, record, session }: ActivityStageProps) {
  const { colors } = useTheme();
  const server = useServer(serverKey);
  const members = useVoiceMembers(serverKey, channelId);
  const menu = useRef<MenuHandle>(null);
  return (
    <View style={styles.stage} testID="activity-stage">
      <ScrollView horizontal style={styles.strip} contentContainerStyle={styles.stripBody} showsHorizontalScrollIndicator={false}>
        {members.map((member) => (
          <ParticipantTile
            key={member.userId}
            serverKey={serverKey}
            channelId={channelId}
            member={member}
            width={STRIP_TILE}
            height={STRIP_HEIGHT}
            avatar={28}
          />
        ))}
      </ScrollView>
      <View style={[styles.bar, { borderColor: colors.border }]}>
        <PluginIcon serverKey={serverKey} pluginId={record.pluginId} icon={record.icon} size={24} radius={6} />
        <Text variant="headline" numberOfLines={1} style={styles.name}>
          {record.name}
        </Text>
        <Menu ref={menu} actions={activityActions(server, record, true)} onAction={(id) => runActivityAction(serverKey, record, id)} trigger="press">
          <IconButton icon="more" label="More" size={36} onPress={() => menu.current?.open()} />
        </Menu>
        <IconButton icon="close" label="Close" size={36} onPress={closeOpenActivity} />
      </View>
      <View style={styles.area}>
        <PluginArea
          session={session}
          pluginId={record.pluginId}
          paused={false}
          onLeave={closeOpenActivity}
          onReload={() => {
            closeOpenActivity();
            openActivity(serverKey, channelId, record.id);
          }}
        />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  stage: { flex: 1 },
  strip: { flexGrow: 0, height: STRIP_HEIGHT + space.sm * 2 },
  stripBody: { gap: space.sm, paddingHorizontal: space.md, paddingVertical: space.sm },
  bar: { flexDirection: 'row', alignItems: 'center', gap: space.sm, paddingHorizontal: space.md, paddingVertical: space.xs, borderBottomWidth: StyleSheet.hairlineWidth },
  name: { flex: 1 },
  area: { flex: 1 },
});
