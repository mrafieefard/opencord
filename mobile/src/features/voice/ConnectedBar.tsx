// The connected voice bar (spec §8.6): a 56 pt bar showing current voice
// connection (channel, server or currently playing music) with mute and
// leave buttons. A tap on the bar opens the call (openCall), and so does
// pulling it up: it lifts with the finger, as Apple Maps' sheet does.
import { useCallback } from 'react';
import { StyleSheet, View } from 'react-native';
import { Gesture, GestureDetector, Touchable } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withSpring } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { scheduleOnRN } from 'react-native-worklets';

import { report } from '@/core';
import { leaveVoice, setMuted } from '@/state/actions/voice';
import { useMyVoice, useNowPlaying, useServer } from '@/state/selectors';
import { Icon, IconButton, space, Text, useTheme } from '@/ui';

import { openCall } from './openCall';

/** How far a pull upwards opens the call, and how fast a flick does. */
const PULL = 36;
const FLICK = -600;
/** The bar follows a pull by this share, up to LIFT_MAX. */
const FOLLOW = 0.4;
const LIFT_MAX = 28;

export interface ConnectedBarProps {
  /** The server whose screen this is; none on Servers. */
  serverKey?: string;
  /** The channel open beside it: the bar hides when that is the voice channel. */
  channelId?: string;
  /** At the very bottom of the screen, the bar stands on the bottom inset. */
  bottomInset?: boolean;
}

const BAR_HEIGHT = 56;

export function ConnectedBar({ serverKey, channelId, bottomInset = false }: ConnectedBarProps) {
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
      bottomInset={bottomInset}
    />
  );
}

interface ConnectedBarContentProps {
  serverKey: string;
  channelId: string;
  muted: boolean;
  bottomInset: boolean;
}

function ConnectedBarContent({ serverKey, channelId, muted, bottomInset }: ConnectedBarContentProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
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
      <ConnectedVoiceRow serverKey={serverKey} channelId={channelId} muted={muted} />
    </View>
  );
}

export interface ConnectedVoiceRowProps {
  serverKey: string;
  channelId: string;
  muted: boolean;
  /** Only the channel and the mic, for a narrow place (iOS's inline tab accessory). */
  compact?: boolean;
}

/** What the connected bar shows, without the bar: also iOS's tab bar accessory. */
export function ConnectedVoiceRow({ serverKey, channelId, muted, compact = false }: ConnectedVoiceRowProps) {
  const server = useServer(serverKey);
  const nowPlaying = useNowPlaying(serverKey, channelId);

  const serverName = server?.snapshot?.name ?? server?.entry.name ?? 'Opencord';
  const channel = server?.snapshot?.channels.find((c) => c.id === channelId);
  const channelName = channel?.name ?? 'Voice';

  const isPlayingMusic = nowPlaying && !nowPlaying.ended && !!nowPlaying.title;
  const subtitle = isPlayingMusic
    ? `♪ ${nowPlaying.title}${nowPlaying.artist ? ` — ${nowPlaying.artist}` : ''}`
    : serverName;

  const handleOpen = useCallback(() => openCall(serverKey, channelId), [serverKey, channelId]);

  // Pulled up, the bar lifts with the finger; far or fast enough, the call opens.
  const lift = useSharedValue(0);
  const pull = Gesture.Pan()
    .activeOffsetY(-8)
    .failOffsetX([-16, 16])
    .onUpdate((event) => {
      lift.set(Math.max(Math.min(event.translationY, 0) * FOLLOW, -LIFT_MAX));
    })
    .onEnd((event) => {
      if (event.translationY < -PULL || event.velocityY < FLICK) scheduleOnRN(handleOpen);
    })
    .onFinalize(() => {
      lift.set(withSpring(0, { damping: 18, stiffness: 260 }));
    });
  const liftStyle = useAnimatedStyle(() => ({ transform: [{ translateY: lift.get() }] }));

  return (
    <GestureDetector gesture={pull}>
    <Animated.View style={[styles.row, liftStyle]}>
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
            {!compact && (
              <Text variant="footnote" color="textSecondary" numberOfLines={1}>
                {subtitle}
              </Text>
            )}
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
        {!compact && (
          <IconButton
            icon="leave"
            label="Leave voice"
            onPress={() => report(leaveVoice(), 'leave voice')}
            testID="connected-bar-leave"
          />
        )}
      </View>
    </Animated.View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    alignItems: 'center',
    borderTopWidth: StyleSheet.hairlineWidth,
    paddingHorizontal: space.sm,
  },
  row: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
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
