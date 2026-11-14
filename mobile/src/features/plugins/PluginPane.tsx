// A plugin channel's pane (plugins M4 plan, Task 12): the app's pane header,
// then the plugin's screen down to the safe area. One view session per open
// pane: leaving the channel closes the view (§14.3). The sidebar over it
// pauses it; a channel that comes back while open opens afresh.
import { ChannelKind, PluginChannelState } from '@opencord/core-native';
import { router } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { chrome } from '@/features/servers/chrome';
import { ConnectedBar } from '@/features/voice/ConnectedBar';
import { useMyVoice, useServer } from '@/state/selectors';
import { PaneHeader, Text, space, useTheme } from '@/ui';

import { PluginArea } from './PluginArea';
import { usePluginSession } from './usePluginSession';

export interface PluginPaneProps {
  serverKey: string;
  channelId: string;
  onOpenSidebar?: () => void;
  onOpenChannel?: (channelId: string) => void;
  /** The sidebar is over the pane. */
  covered?: boolean;
  /** Back with nothing left for the plugin to close; a tablet leaves the server. */
  onBack?: () => void;
}

const leaveServer = () => router.back();

/** A counter that goes up each time `on` turns true (React's state-from-props pattern). */
function useTimesOn(on: boolean): number {
  const [last, setLast] = useState(on);
  const [times, setTimes] = useState(0);
  if (on !== last) {
    setLast(on);
    if (on) setTimes(times + 1);
  }
  return times;
}

export function PluginPane({ serverKey, channelId, onOpenSidebar, onOpenChannel, covered = false, onBack }: PluginPaneProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const channel = useServer(serverKey)?.snapshot?.channels.find((candidate) => candidate.id === channelId);
  const plugin = channel?.kind === ChannelKind.Plugin ? channel.plugin : undefined;
  const available = plugin?.state === PluginChannelState.Available;
  const [reloads, setReloads] = useState(0);
  const comebacks = useTimesOn(available);
  const place = useMemo(() => (available ? { kind: 'channel' as const, serverKey, channelId } : null), [available, serverKey, channelId]);
  const openChannel = useCallback((id: string) => onOpenChannel?.(id), [onOpenChannel]);
  const myVoice = useMyVoice();
  const showBar = onOpenChannel !== undefined && myVoice !== null && chrome.serversVoiceBar;
  const session = usePluginSession({
    place,
    pluginId: plugin?.pluginId ?? '',
    openChannel,
    generation: reloads + comebacks * 1000,
  });

  return (
    <View style={[styles.root, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={channel?.name ?? ''}
        titleIcon="extension"
        left={onOpenSidebar ? { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar } : undefined}
      />
      <View style={[styles.body, { paddingBottom: showBar ? 0 : insets.bottom }]}>
        {session ? (
          <PluginArea
            session={session}
            pluginId={plugin?.pluginId ?? ''}
            paused={covered}
            onLeave={onBack ?? leaveServer}
            onReload={() => setReloads((count) => count + 1)}
          />
        ) : (
          !available && (
            <View style={styles.center}>
              <Text variant="headline">This channel is unavailable</Text>
            </View>
          )
        )}
      </View>
      {/* The call this device is in elsewhere, as a voice pane shows it (Android). */}
      {showBar && (
        <View style={{ paddingBottom: insets.bottom }}>
          <ConnectedBar serverKey={serverKey} channelId={channelId} />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
  body: { flex: 1 },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: space.lg },
});
