// An activity's own page (showActivity): the plugin's screen, full size,
// under a header with its name, the call it's in, a way back to the call and
// its menu (Close, End activity). The people in the call aren't repeated
// here; the call's control bar brings the page back. Going back keeps the
// activity open, paused; Close, its end, or leaving the call closes it, and
// the page goes with it.
import { router } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';

import { openActivity } from '@/state/actions/plugins';
import { useChannelInfo, useServer } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { IconButton, Menu, PaneHeader, useTheme, type MenuHandle } from '@/ui';

import { activityActions, runActivityAction } from './activityActions';
import { STAGE_PAGE, claimStage, closeOpenActivity, useOpenActivity } from './activityHolder';
import { PluginArea } from './PluginArea';

export interface ActivityPageProps {
  serverKey: string;
  sessionId: string;
}

function back() {
  if (router.canGoBack()) router.back();
}

export function ActivityPage({ serverKey, sessionId }: ActivityPageProps) {
  const { colors } = useTheme();
  const server = useServer(serverKey);
  const live = server?.snapshot?.activitySessions.find((s) => s.id === sessionId);
  const held = useOpenActivity();
  const mine = held?.serverKey === serverKey && held.sessionId === sessionId ? held : null;
  // Kept while it says it ended, after it left the snapshot.
  const record = mine?.record ?? live;
  const call = useChannelInfo(serverKey, record?.channelId ?? '');
  const menu = useRef<MenuHandle>(null);

  // Reached without showActivity (a restored screen), it opens itself.
  useEffect(() => {
    const target = useAppStore.getState().openActivity;
    if (live && (target?.serverKey !== serverKey || target.sessionId !== sessionId)) {
      openActivity(serverKey, live.channelId, sessionId);
    }
    // Once, as the page comes up.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // It draws here while the page is up.
  const [claim] = useState(() => Symbol('activity page'));
  useEffect(() => claimStage(claim, STAGE_PAGE), [claim]);

  // Closed (Close, its end, leaving the call, another opened): back to the call.
  const ours = useAppStore((state) => state.openActivity?.serverKey === serverKey && state.openActivity.sessionId === sessionId);
  const wasOurs = useRef(ours);
  useEffect(() => {
    if (ours) wasOurs.current = true;
    else if (wasOurs.current) back();
  }, [ours]);

  return (
    <View style={[styles.page, { backgroundColor: colors.chat }]} testID="activity-page">
      <PaneHeader
        title={record?.name ?? ''}
        subtitle={call?.channel.name}
        left={{ icon: 'dismiss', label: 'Back to the call', onPress: back }}
        rightElement={
          record ? (
            <Menu
              ref={menu}
              actions={activityActions(server, record, true)}
              onAction={(id) => runActivityAction(serverKey, record, id)}
              trigger="press"
            >
              <IconButton icon="more" label="More" onPress={() => menu.current?.open()} />
            </Menu>
          ) : undefined
        }
      />
      <View style={styles.area}>
        {mine ? (
          <PluginArea
            session={mine.session}
            pluginId={mine.pluginId}
            paused={false}
            onLeave={back}
            onReload={() => {
              const channelId = mine.channelId;
              closeOpenActivity();
              openActivity(serverKey, channelId, sessionId);
            }}
          />
        ) : (
          <View style={styles.waiting}>
            <ActivityIndicator />
          </View>
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
  area: { flex: 1 },
  waiting: { flex: 1, alignItems: 'center', justifyContent: 'center' },
});
