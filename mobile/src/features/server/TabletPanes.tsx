// The server screen from 768 pt wide (spec §7.2): no drawer. The channel
// list is a fixed 320 pt sidebar and the open channel fills the rest.
import { router } from 'expo-router';
import { StyleSheet, View } from 'react-native';

import { chrome } from '@/features/servers/chrome';
import { ConnectedBar } from '@/features/voice/ConnectedBar';
import { useTheme } from '@/ui';

import { ChannelList } from './ChannelList';
import { ChannelListHeader } from './ChannelListHeader';
import { ContentPane } from './ContentPane';
import type { ServerPanes } from './useServerPanes';

export const TABLET_SIDEBAR_WIDTH = 320;

const leave = () => router.back();

export interface TabletPanesProps {
  serverKey: string;
  panes: ServerPanes;
}

export function TabletPanes({ serverKey, panes }: TabletPanesProps) {
  const { colors } = useTheme();
  return (
    <View style={styles.row}>
      <View style={[styles.sidebar, { borderRightColor: colors.border }]}>
        <ChannelListHeader serverKey={serverKey} backLabel="Servers" onBack={leave} />
        <ChannelList serverKey={serverKey} variant="sidebar" selectedId={panes.open?.id} onSelect={panes.select} />
        {chrome.serversVoiceBar && (
          <ConnectedBar serverKey={serverKey} channelId={panes.open?.id} bottomInset />
        )}
      </View>
      <View style={styles.content}>
        <ContentPane serverKey={serverKey} open={panes.open} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flex: 1, flexDirection: 'row' },
  sidebar: { width: TABLET_SIDEBAR_WIDTH, borderRightWidth: StyleSheet.hairlineWidth },
  content: { flex: 1 },
});
