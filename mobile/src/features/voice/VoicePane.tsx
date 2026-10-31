// A voice channel's pane (spec §8.6): its header, then who is in it.
// Connected: tiles, link banner, control bar.
// Not connected: tiles, Join voice button (or disabled reason).
import { AudioRouteButton, hasAudioRouteButton } from '@opencord/voice-service';
import { router } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import { type LayoutChangeEvent, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Animated, { useAnimatedScrollHandler, useSharedValue } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';
import { useMusicSettingsLoader } from '@/features/music/useMusicSettingsLoader';
import { joinVoice, leaveVoice, setDeafened, setMuted } from '@/state/actions/voice';
import { useChannelInfo, useMusicSettings, useMyVoice, useVoiceMembers } from '@/state/selectors';
import { Button, EmptyState, IconButton, PaneHeader, space, useTheme } from '@/ui';

import { chrome } from '@/features/servers/chrome';

import { CALL_MARGIN } from './callInset';
import { callDrag, closeCall, useCallTop } from './callPosition';
import { ConnectedBar } from './ConnectedBar';
import { ControlBar } from './ControlBar';
import { LinkBanner } from './LinkBanner';
import { ParticipantTile } from './ParticipantTile';
import { tileGrid } from './tileGrid';

export interface VoicePaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
  /** Given, the connected bar sits at the bottom while in voice elsewhere, and opens channels with it. */
  onOpenChannel?: (channelId: string) => void;
  /** In the call sheet (CallSheet): a chevron to put it away for the sidebar button. */
  asCall?: boolean;
  /** The sheet's drag, on the header too: dragging it down takes the sheet down. */
  headerGesture?: ReturnType<typeof Gesture.Pan>;
}

const GAP = space.md;
const PADDING = space.md;

export function VoicePane({ serverKey, channelId, onOpenSidebar, onOpenChannel, asCall = false, headerGesture }: VoicePaneProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { width: windowWidth, height: windowHeight } = useWindowDimensions();
  // The grid's room, as it lays out: the tiles fill it.
  const [room, setRoom] = useState({ width: windowWidth, height: windowHeight / 2 });

  const info = useChannelInfo(serverKey, channelId);
  useMusicSettingsLoader(serverKey);
  const musicSettings = useMusicSettings(serverKey);
  const myVoice = useMyVoice();
  const members = useVoiceMembers(serverKey, channelId);

  const here = myVoice?.serverKey === serverKey && myVoice.channelId === channelId ? myVoice : null;
  const showMusic = !!(musicSettings?.supported && musicSettings.enabled);

  // The user's own tile first.
  const sorted = useMemo(
    () => [...members].sort((a, b) => Number(b.isSelf) - Number(a.isSelf) || a.name.localeCompare(b.name)),
    [members],
  );

  const join = useCallback(() => report(joinVoice(serverKey, channelId), 'join voice'), [serverKey, channelId]);
  const leave = useCallback(() => report(leaveVoice(), 'leave voice'), []);
  const openMusic = useCallback(() => router.push(`/server/${serverKey}/music/${channelId}`), [serverKey, channelId]);
  const openSoundboard = useCallback(() => router.push(`/server/${serverKey}/soundboard/${channelId}`), [serverKey, channelId]);
  const openMember = useCallback((userId: string) => router.push(`/server/${serverKey}/member/${userId}`), [serverKey]);
  // In the call, the grid scrolls only when not everyone fits; a pull down
  // at its top (or anywhere, when it doesn't scroll) takes the call down.
  const callTop = useCallTop();
  const scrollY = useSharedValue(0);
  const onScroll = useAnimatedScrollHandler((event) => {
    scrollY.set(event.contentOffset.y);
  });
  const gridDrag = useMemo(
    () => (asCall ? Gesture.Simultaneous(callDrag(callTop, windowHeight, scrollY), Gesture.Native()) : null),
    [asCall, callTop, windowHeight, scrollY],
  );
  // The call floats above the screen's bottom edge (callInset): less of the
  // home indicator's room is its own.
  const bottom = asCall ? Math.max(insets.bottom - CALL_MARGIN, space.md) : insets.bottom;

  const onLayout = useCallback((event: LayoutChangeEvent) => {
    const { width, height } = event.nativeEvent.layout;
    if (width > 0 && height > 0) setRoom({ width, height });
  }, []);

  const grid = tileGrid(sorted.length, room.width - PADDING * 2, room.height - PADDING * 2, GAP);
  const userLimit = info?.channel.userLimit ?? 0;
  const reason =
    info?.access.connect === false
      ? "You can't join this channel"
      : userLimit > 0 && members.length >= userLimit
        ? 'This channel is full'
        : null;

  const header = (
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="voiceChannel"
        insetTop={!asCall}
        clear={asCall}
        left={
          asCall
            ? { icon: 'dismiss', label: 'Close', onPress: closeCall }
            : onOpenSidebar
              ? { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar }
              : undefined
        }
        rightElement={
          // In the call, the sound output (iOS: speaker, iPhone, AirPods…) beside Music.
          (here && hasAudioRouteButton) || showMusic ? (
            <>
              {here && hasAudioRouteButton && <AudioRouteButton testID="control-output" tint={colors.text} style={styles.route} />}
              {showMusic && <IconButton icon="music" label="Music" onPress={openMusic} />}
            </>
          ) : undefined
        }
      />
  );

  const tiles = (
    <Animated.ScrollView
      style={styles.fill}
      contentContainerStyle={styles.grid}
      onLayout={onLayout}
      onScroll={onScroll}
      scrollEventThrottle={16}
      scrollEnabled={grid.scrolls}
      // At its top a pull takes the call down instead of stretching the grid.
      bounces={!asCall}
    >
      {sorted.length === 0 ? (
        <EmptyState title="No one's here yet" />
      ) : (
        <View style={styles.tiles}>
          {sorted.map((member, index) => (
            <ParticipantTile
              key={member.userId}
              serverKey={serverKey}
              channelId={channelId}
              member={member}
              width={index === sorted.length - 1 ? grid.lastWidth : grid.tileWidth}
              height={grid.tileHeight}
              avatar={grid.avatar}
              onPress={() => openMember(member.userId)}
            />
          ))}
        </View>
      )}
    </Animated.ScrollView>
  );

  return (
    // The call shows its glass through (CallSheet); a pane is the chat's colour.
    <View style={[styles.pane, { backgroundColor: asCall ? 'transparent' : colors.chat }]}>
      {headerGesture ? (
        <GestureDetector gesture={headerGesture}>
          <View collapsable={false}>{header}</View>
        </GestureDetector>
      ) : (
        header
      )}
      {here && <LinkBanner link={here.link} onRejoin={join} />}
      {gridDrag ? (
        <GestureDetector gesture={gridDrag}>{tiles}</GestureDetector>
      ) : (
        tiles
      )}
      {here ? (
        <View style={{ paddingBottom: bottom }}>
          <ControlBar
            muted={here.muted}
            deafened={here.deafened}
            onToggleMute={() => setMuted(!here.muted)}
            onToggleDeafen={() => setDeafened(!here.deafened)}
            onOpenSoundboard={openSoundboard}
            onLeave={leave}
          />
        </View>
      ) : (
        <>
          <View style={styles.join}>
            <Button title={reason ?? 'Join voice'} variant="voice" disabled={reason !== null} onPress={join} />
          </View>
          {/* iOS shows the call above the tab bar instead (chrome.ts). */}
          {onOpenChannel && myVoice && chrome.serversVoiceBar ? (
            <ConnectedBar serverKey={serverKey} channelId={channelId} bottomInset />
          ) : (
            <View style={{ height: insets.bottom }} />
          )}
        </>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
  fill: { flex: 1 },
  grid: { flexGrow: 1, padding: PADDING },
  tiles: { flexDirection: 'row', flexWrap: 'wrap', gap: GAP },
  join: { padding: space.md },
  route: { width: 44, height: 44 },
});
