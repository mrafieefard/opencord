// The server screen from 768 pt wide (spec §7.2): no drawer. The channel
// list is a fixed 320 pt sidebar and the open channel fills the rest.
import { router } from 'expo-router';
import { StyleSheet, useWindowDimensions, View } from 'react-native';

import { MembersSheet } from '@/features/members/MembersSheet';
import { chrome } from '@/features/servers/chrome';
import { ConnectedBar } from '@/features/voice/ConnectedBar';
import { useTheme } from '@/ui';

import { ChannelList } from './ChannelList';
import { ChannelListHeader } from './ChannelListHeader';
import { ContentPane } from './ContentPane';
import type { ServerPanes } from './useServerPanes';

export const TABLET_SIDEBAR_WIDTH = 320;
export const TABLET_MEMBERS_WIDTH = 280;

const leave = () => router.back();

export interface TabletPanesProps {
  serverKey: string;
  panes: ServerPanes;
}

export function TabletPanes({ serverKey, panes }: TabletPanesProps) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const showMembers = width >= 1000;

  return (
    <View style={styles.row}>
      <View style={[styles.sidebar, { borderRightColor: colors.border }]}>
        <ChannelListHeader serverKey={serverKey} backLabel="Servers" onBack={leave} />
        <ChannelList serverKey={serverKey} variant="sidebar" selectedId={panes.open?.id} onSelect={panes.select} />
        {chrome.serversVoiceBar && (
          <ConnectedBar serverKey={serverKey} channelId={panes.open?.id} />
        )}
      </View>
      <View style={styles.content}>
        <ContentPane serverKey={serverKey} open={panes.open} />
      </View>
      {showMembers && (
        <View style={[styles.members, { borderLeftColor: colors.border, backgroundColor: colors.chat }]}>
          <MembersSheet serverKey={serverKey} variant="column" />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flex: 1, flexDirection: 'row' },
  sidebar: { width: TABLET_SIDEBAR_WIDTH, borderRightWidth: StyleSheet.hairlineWidth },
  content: { flex: 1 },
  members: { width: TABLET_MEMBERS_WIDTH, borderLeftWidth: StyleSheet.hairlineWidth },
});
