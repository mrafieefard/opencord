// The connected voice bar (spec §8.6): a 56 pt bar showing current voice
// connection (channel, server or currently playing music) with mute and
// leave buttons. A tap on the bar opens that voice channel.
import { router } from 'expo-router';
import { useCallback } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { leaveVoice, setMuted } from '@/state/actions/voice';
import { useMyVoice, useNowPlaying, useServer } from '@/state/selectors';
import { Icon, IconButton, space, Text, useTheme } from '@/ui';

export interface ConnectedBarProps {
  serverKey?: string;
  channelId?: string;
  onPress?: () => void;
}

const BAR_HEIGHT = 56;

export function ConnectedBar({ serverKey, channelId, onPress }: ConnectedBarProps) {
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
      onPress={onPress}
    />
  );
}

interface ConnectedBarContentProps {
  serverKey: string;
  channelId: string;
  muted: boolean;
  onPress?: () => void;
}

function ConnectedBarContent({ serverKey, channelId, muted, onPress }: ConnectedBarContentProps) {
  const { colors } = useTheme();
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
    if (onPress) {
      onPress();
      return;
    }
    router.push({
      pathname: '/server/[serverKey]',
      params: { serverKey, channel: channelId },
    });
  }, [serverKey, channelId, onPress]);

  return (
    <View
      style={[
        styles.bar,
        {
          backgroundColor: colors.surface,
          borderTopColor: colors.border,
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
          onPress={leaveVoice}
          testID="connected-bar-leave"
        />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    height: BAR_HEIGHT,
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
