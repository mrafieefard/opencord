// The connected voice bar (spec §8.6): a 56 pt bar showing current voice
// connection (channel, server or currently playing music) with mute and
// leave buttons. A tap on the bar opens that voice channel: in place on
// its own server's screen, else on top as that server's screen.
import { router } from 'expo-router';
import { useCallback } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';
import { leaveVoice, setMuted } from '@/state/actions/voice';
import { useMyVoice, useNowPlaying, useServer } from '@/state/selectors';
import { Icon, IconButton, space, Text, useTheme } from '@/ui';

export interface ConnectedBarProps {
  /** The server whose screen this is; none on Servers. */
  serverKey?: string;
  /** The channel open beside it: the bar hides when that is the voice channel. */
  channelId?: string;
  /** Opens a channel of `serverKey`'s screen in place. */
  onOpenChannel?: (channelId: string) => void;
  /** At the very bottom of the screen, the bar stands on the bottom inset. */
  bottomInset?: boolean;
}

const BAR_HEIGHT = 56;

export function ConnectedBar({ serverKey, channelId, onOpenChannel, bottomInset = false }: ConnectedBarProps) {
  const myVoice = useMyVoice();

  // Hidden when not connected, or when already viewing that channel
  if (!myVoice) return null;
  if (channelId && myVoice.serverKey === serverKey && myVoice.channelId === channelId) {
    return null;
  }

  return (
    <ConnectedBarContent
      serverKey={myVoice.serverKey}
      channelId={myVoice.channelId}
      muted={myVoice.muted}
      onOpenHere={myVoice.serverKey === serverKey ? onOpenChannel : undefined}
      bottomInset={bottomInset}
    />
  );
}

interface ConnectedBarContentProps {
  serverKey: string;
  channelId: string;
  muted: boolean;
  onOpenHere?: (channelId: string) => void;
  bottomInset: boolean;
}

function ConnectedBarContent({ serverKey, channelId, muted, onOpenHere, bottomInset }: ConnectedBarContentProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const server = useServer(serverKey);
  const nowPlaying = useNowPlaying(serverKey, channelId);

  const serverName = server?.snapshot?.name ?? server?.entry.name ?? 'Opencord';
  const channel = server?.snapshot?.channels.find((c) => c.id === channelId);
  const channelName = channel?.name ?? 'Voice';

  const isPlayingMusic = nowPlaying && !nowPlaying.ended && !!nowPlaying.title;
  const subtitle = isPlayingMusic
    ? `♪ ${nowPlaying.title}${nowPlaying.artist ? ` — ${nowPlaying.artist}` : ''}`
    : serverName;

  const handleOpen = useCallback(() => {
    if (onOpenHere) {
      onOpenHere(channelId);
      return;
    }
    router.push({
      pathname: '/server/[serverKey]',
      params: { serverKey, channel: channelId },
    });
  }, [serverKey, channelId, onOpenHere]);

  return (
    <View
      style={[
        styles.bar,
        {
          backgroundColor: colors.surface,
          borderTopColor: colors.border,
          height: BAR_HEIGHT + (bottomInset ? insets.bottom : 0),
          paddingBottom: bottomInset ? insets.bottom : 0,
        },
      ]}
    >
      <Touchable
        onPress={handleOpen}
        style={styles.pressable}
        accessible
        accessibilityRole="button"
        accessibilityLabel={`In voice: ${channelName}, ${subtitle}`}
        testID="connected-bar-pressable"
      >
        <View style={styles.left}>
          <Icon name="voiceChannel" size={20} color="voice" />
          <View style={styles.labels}>
            <Text variant="headline" numberOfLines={1}>
              {channelName}
            </Text>
            <Text variant="footnote" color="textSecondary" numberOfLines={1}>
              {subtitle}
            </Text>
          </View>
        </View>
      </Touchable>

      <View style={styles.actions}>
        <IconButton
          icon={muted ? 'micOff' : 'mic'}
          label={muted ? 'Unmute' : 'Mute'}
          onPress={() => setMuted(!muted)}
          testID="connected-bar-mute"
        />
        <IconButton
          icon="leave"
          label="Leave voice"
          onPress={() => report(leaveVoice(), 'leave voice')}
          testID="connected-bar-leave"
        />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    alignItems: 'center',
    borderTopWidth: StyleSheet.hairlineWidth,
    paddingHorizontal: space.sm,
  },
  pressable: {
    flex: 1,
  },
  left: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingVertical: space.xs,
  },
  labels: {
    flex: 1,
  },
  actions: {
    flexDirection: 'row',
    alignItems: 'center',
  },
});
