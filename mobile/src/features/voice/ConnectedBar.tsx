// The connected voice bar (spec §8.6): a 56 pt bar showing current voice
// connection (channel, server or currently playing music) with mute and
// leave buttons. A tap on the bar opens the call (callPosition.ts), and
// pulling it up brings the call's sheet up under the finger, as Apple
// Maps' sheet comes up, springing open or back on letting go.
import { type ComponentRef, useCallback, useRef } from 'react';
import { StyleSheet, View, useWindowDimensions } from 'react-native';
import { Gesture, GestureDetector, Touchable } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, withSpring } from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

import { report } from '@/core';
import { leaveVoice, setMuted } from '@/state/actions/voice';
import { useMyVoice, useNowPlaying, useServer } from '@/state/selectors';
import { Icon, IconButton, space, Text, useTheme } from '@/ui';

import {
  BAR_HEIGHT,
  BAR_PAD_LEFT,
  BAR_PAD_RIGHT,
  CALL_FLICK,
  CALL_SETTLE,
  CALL_SPRING,
  barFrame,
  callOrigin,
  callOriginRound,
  callProgress,
  callTravel,
  openCall,
  showCall,
  useCallTop,
} from './callPosition';

export interface ConnectedBarProps {
  /** The server whose screen this is; none on Servers. */
  serverKey?: string;
  /** The channel open beside it: the bar hides when that is the voice channel. */
  channelId?: string;
}

export function ConnectedBar({ serverKey, channelId }: ConnectedBarProps) {
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
    />
  );
}

interface ConnectedBarContentProps {
  serverKey: string;
  channelId: string;
  muted: boolean;
}

// Android's screens end above the system's navigation bar, so the bar
// never stands on a bottom inset: above the tab bar, or at a pane's foot.
function ConnectedBarContent({ serverKey, channelId, muted }: ConnectedBarContentProps) {
  const { colors } = useTheme();
  // The call's glass grows out of it: once it does, the glass is the bar.
  const away = useAnimatedStyle(() => ({ opacity: callProgress.get() > 0.001 ? 0 : 1 }));
  return (
    <Animated.View
      style={[
        away,
        styles.bar,
        {
          backgroundColor: colors.surface,
          borderTopColor: colors.border,
        },
      ]}
    >
      <ConnectedVoiceRow serverKey={serverKey} channelId={channelId} muted={muted} square />
    </Animated.View>
  );
}

export interface ConnectedVoiceRowProps {
  serverKey: string;
  channelId: string;
  muted: boolean;
  /** In the call's own glass, which knows where it is: the row doesn't measure itself. */
  inGlass?: boolean;
  /** Only the channel and the mic, for a narrow place. */
  compact?: boolean;
  /** In Android's edge-to-edge bar, not a pill: the call grows from its square corners. */
  square?: boolean;
}

/** What the connected bar shows, without the bar: also iOS's floating pill (CallSheet). */
export function ConnectedVoiceRow({
  serverKey,
  channelId,
  muted,
  compact = false,
  inGlass = false,
  square = false,
}: ConnectedVoiceRowProps) {
  const server = useServer(serverKey);
  const nowPlaying = useNowPlaying(serverKey, channelId);

  const serverName = server?.snapshot?.name ?? server?.entry.name ?? 'Opencord';
  const channel = server?.snapshot?.channels.find((c) => c.id === channelId);
  const channelName = channel?.name ?? 'Voice';

  const isPlayingMusic = nowPlaying && !nowPlaying.ended && !!nowPlaying.title;
  const subtitle = isPlayingMusic
    ? `♪ ${nowPlaying.title}${nowPlaying.artist ? ` — ${nowPlaying.artist}` : ''}`
    : serverName;

  // The call grows out of this bar's glass: where it is, measured as it lays
  // out and again the moment a finger lands on it, before a pull or a tap acts.
  const rowRef = useRef<ComponentRef<typeof View>>(null);
  const top = useCallTop();
  const { height: screen } = useWindowDimensions();
  const measureBar = useCallback(() => {
    if (inGlass) return;
    rowRef.current?.measureInWindow((x, y, width, height) => {
      if (width === 0) return;
      callOrigin.set(barFrame({ x, y, width, height }));
      callOriginRound.set(!square);
    });
  }, [inGlass, square]);
  const handleOpen = useCallback(() => openCall(serverKey, channelId), [serverKey, channelId]);

  // Pulled up, the glass grows under the finger; far or fast enough it
  // springs open, else back into the bar.
  const pull = Gesture.Pan()
    .activeOffsetY(-8)
    .failOffsetX([-16, 16])
    .onStart(() => {
      scheduleOnRN(showCall, serverKey, channelId);
    })
    .onUpdate((event) => {
      const travel = callTravel(callOrigin.get(), top, screen);
      callProgress.set(Math.min(Math.max(-event.translationY / travel, 0), 1));
    })
    .onEnd((event) => {
      const travel = callTravel(callOrigin.get(), top, screen);
      const open = -event.translationY > travel * CALL_SETTLE || event.velocityY < -CALL_FLICK;
      callProgress.set(withSpring(open ? 1 : 0, { ...CALL_SPRING, velocity: -event.velocityY / travel }));
    });

  return (
    <GestureDetector gesture={pull}>
    <View ref={rowRef} collapsable={false} style={styles.row} onTouchStart={measureBar} onLayout={measureBar}>
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
    </View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  // Inset as the pill's row (barFrame), so the call grows from the whole bar.
  bar: {
    height: BAR_HEIGHT,
    flexDirection: 'row',
    alignItems: 'center',
    borderTopWidth: StyleSheet.hairlineWidth,
    paddingLeft: BAR_PAD_LEFT,
    paddingRight: BAR_PAD_RIGHT,
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
