// A voice channel's pane (spec §8.6): its header, then who is in it.
// Connected: tiles, link banner, control bar.
// Not connected: tiles, Join voice button (or disabled reason).
import { AudioRouteButton, hasAudioRouteButton } from '@opencord/voice-service';
import { router } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import { type LayoutChangeEvent, ScrollView, StyleSheet, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';
import { useMusicSettingsLoader } from '@/features/music/useMusicSettingsLoader';
import { joinVoice, leaveVoice, setDeafened, setMuted } from '@/state/actions/voice';
import { useChannelInfo, useMusicSettings, useMyVoice, useVoiceMembers } from '@/state/selectors';
import { Button, EmptyState, IconButton, PaneHeader, space, useTheme } from '@/ui';

import { chrome } from '@/features/servers/chrome';

import { ConnectedBar } from './ConnectedBar';
import { ControlBar } from './ControlBar';
import { LinkBanner } from './LinkBanner';
import { ParticipantTile } from './ParticipantTile';

export interface VoicePaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
  /** Given, the connected bar sits at the bottom while in voice elsewhere, and opens channels with it. */
  onOpenChannel?: (channelId: string) => void;
}

/** Tiles are at most this wide, so a wider pane has more columns: two on a phone, more on a tablet. */
const TILE_MAX = 200;
const MIN_COLUMNS = 2;
const GAP = space.md;
const PADDING = space.md;

export function tileLayout(width: number): { columns: number; size: number } {
  const inner = width - PADDING * 2;
  const columns = Math.max(MIN_COLUMNS, Math.ceil((inner + GAP) / (TILE_MAX + GAP)));
  return { columns, size: Math.floor((inner - GAP * (columns - 1)) / columns) };
}

export function VoicePane({ serverKey, channelId, onOpenSidebar, onOpenChannel }: VoicePaneProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { width: windowWidth } = useWindowDimensions();
  const [width, setWidth] = useState(windowWidth);

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
  const onLayout = useCallback((event: LayoutChangeEvent) => {
    const next = event.nativeEvent.layout.width;
    if (next > 0) setWidth(next);
  }, []);

  const tile = tileLayout(width);
  const userLimit = info?.channel.userLimit ?? 0;
  const reason =
    info?.access.connect === false
      ? "You can't join this channel"
      : userLimit > 0 && members.length >= userLimit
        ? 'This channel is full'
        : null;

  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="voiceChannel"
        left={onOpenSidebar ? { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar } : undefined}
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
      {here && <LinkBanner link={here.link} onRejoin={join} />}
      <ScrollView style={styles.fill} contentContainerStyle={styles.grid} onLayout={onLayout}>
        {sorted.length === 0 ? (
          <EmptyState title="No one's here yet" />
        ) : (
          <View style={styles.tiles}>
            {sorted.map((member) => (
              <ParticipantTile
                key={member.userId}
                serverKey={serverKey}
                channelId={channelId}
                member={member}
                size={tile.size}
                onPress={() => openMember(member.userId)}
              />
            ))}
          </View>
        )}
      </ScrollView>
      {here ? (
        <View style={{ paddingBottom: insets.bottom }}>
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
            <ConnectedBar serverKey={serverKey} channelId={channelId} onOpenChannel={onOpenChannel} bottomInset />
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
