// A text channel's pane (spec §8.5): its header, then the messages.
import { StyleSheet, View } from 'react-native';

import { useChannelInfo } from '@/state/selectors';
import { PaneHeader, useTheme } from '@/ui';

export interface ChatPaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
}

export function ChatPane({ serverKey, channelId, onOpenSidebar }: ChatPaneProps) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="textChannel"
        subtitle={info?.channel.topic}
        left={onOpenSidebar && { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar }}
      />
      {/* later: Task 13 puts the messages and the composer here. */}
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
});
