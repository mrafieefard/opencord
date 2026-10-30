// A text channel's pane (spec §8.5): its header, then the messages, with
// the dock (the voice bar and the composer) floating over their bottom as
// Telegram's, the messages scrolling under it.
import { useMemo, useState } from 'react';
import { type LayoutChangeEvent, StyleSheet, View } from 'react-native';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useSharedValue } from 'react-native-reanimated';

import { useChannelInfo } from '@/state/selectors';
import { PaneHeader, useTheme } from '@/ui';

import { ChatVoicePill } from './ChatVoicePill';
import { Composer } from './Composer';
import { ChatDockContext, useDockLift } from './dock';
import { MessageList } from './MessageList';

export interface ChatPaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
  onSelectChannel?: (channelId: string) => void;
  /** Given, the voice bar floats above the composer and opens channels with it. */
  onOpenChannel?: (channelId: string) => void;
}

export function ChatPane({ serverKey, channelId, onOpenSidebar, onSelectChannel, onOpenChannel }: ChatPaneProps) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  const lift = useDockLift();
  const padding = useSharedValue(0);
  const [height, setHeight] = useState(0);
  const onDockLayout = (event: LayoutChangeEvent) => {
    const next = Math.round(event.nativeEvent.layout.height);
    padding.set(next);
    setHeight(next);
  };
  const dock = useMemo(() => ({ padding, height }), [padding, height]);

  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="textChannel"
        subtitle={info?.channel.topic}
        left={onOpenSidebar ? { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar } : undefined}
      />
      <ChatDockContext.Provider value={dock}>
        <View style={styles.body}>
          <MessageList serverKey={serverKey} channelId={channelId} onSelectChannel={onSelectChannel} />
          {/* The dock rides the keyboard: it stands on its bottom margin, so it
              rises by the keyboard's height less that margin, and the list
              lifts by the same (MessageList's ChatScrollView). */}
          <KeyboardStickyView offset={{ closed: 0, opened: lift }} style={styles.dock} pointerEvents="box-none">
            <View onLayout={onDockLayout} pointerEvents="box-none">
              {onOpenChannel && <ChatVoicePill serverKey={serverKey} onOpenChannel={onOpenChannel} />}
              <Composer serverKey={serverKey} channelId={channelId} />
            </View>
          </KeyboardStickyView>
        </View>
      </ChatDockContext.Provider>
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
  body: { flex: 1 },
  dock: { position: 'absolute', left: 0, right: 0, bottom: 0 },
});
