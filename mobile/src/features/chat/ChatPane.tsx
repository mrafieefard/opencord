// A text channel's pane (spec §8.5): its header, then the messages.
import { StyleSheet, View } from 'react-native';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { ConnectedBar } from '@/features/voice/ConnectedBar';
import { useChannelInfo } from '@/state/selectors';
import { PaneHeader, useTheme } from '@/ui';

import { MessageList } from './MessageList';
import { Composer } from './Composer';

export interface ChatPaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
  onSelectChannel?: (channelId: string) => void;
  /** Given, the connected bar sits above the composer and opens channels with it. */
  onOpenChannel?: (channelId: string) => void;
}

export function ChatPane({ serverKey, channelId, onOpenSidebar, onSelectChannel, onOpenChannel }: ChatPaneProps) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  const insets = useSafeAreaInsets();

  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="textChannel"
        subtitle={info?.channel.topic}
        left={onOpenSidebar ? { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar } : undefined}
      />
      <MessageList
        serverKey={serverKey}
        channelId={channelId}
        onSelectChannel={onSelectChannel}
      />
      {onOpenChannel && <ConnectedBar serverKey={serverKey} channelId={channelId} onOpenChannel={onOpenChannel} />}
      {/* The composer rides the keyboard. It already stands on the bottom
          inset, so it rises by the keyboard's height less that inset; the
          list lifts by the same amount (MessageList's ChatScrollView). */}
      <KeyboardStickyView offset={{ closed: 0, opened: insets.bottom }}>
        <Composer serverKey={serverKey} channelId={channelId} />
      </KeyboardStickyView>
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
});
