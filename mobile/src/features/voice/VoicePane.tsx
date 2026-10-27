// A voice channel's pane (spec §8.6): its header, then who is in it.
import { StyleSheet, View } from 'react-native';

import { useChannelInfo } from '@/state/selectors';
import { PaneHeader, useTheme } from '@/ui';

export interface VoicePaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
}

export function VoicePane({ serverKey, channelId, onOpenSidebar }: VoicePaneProps) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="voiceChannel"
        left={onOpenSidebar && { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar }}
      />
      {/* later: Task 17 puts the tiles, Join and the controls here. */}
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
});
