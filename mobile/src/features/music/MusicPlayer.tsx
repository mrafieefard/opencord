// The music player (spec §8.7), a full-height sheet. On a phone, one
// scrolling page: what plays, the transport and the volume, then adding
// music, the uploads, Up next and Recently played. Search takes the page
// over, with what plays shrunk to a row above it. Two columns when the
// sheet is wide: the player on the left, the lists on the right.
import { MusicAction, MusicPlayState, MusicRepeat } from '@opencord/core-native';
import * as Clipboard from 'expo-clipboard';
import { router } from 'expo-router';
import { useCallback, useState } from 'react';
import { type LayoutChangeEvent, ScrollView, StyleSheet, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';
import { playback, setMode, voteSkip } from '@/state/actions/music';
import { setMusicListening } from '@/state/actions/prefs';
import { toast } from '@/state/actions/ui';
import { joinVoice } from '@/state/actions/voice';
import { useChannelInfo, useMusicSession, useMusicSettings, useMyVoice, usePrefs, useServer } from '@/state/selectors';
import { type MenuAction, SheetGrabber, Text, space, useTheme } from '@/ui';

import { AddBar } from './AddBar';
import { Artwork } from './Artwork';
import { History } from './History';
import { MiniPlayer } from './MiniPlayer';
import { musicAccess } from './musicAccess';
import { MusicSearch } from './MusicSearch';
import { NowPlaying } from './NowPlaying';
import { PlayerControls } from './PlayerControls';
import { PlayerTopBar } from './PlayerTopBar';
import { positionAt } from './position';
import { GUTTER } from './TrackRow';
import { UpNext } from './UpNext';
import { Uploads } from './Uploads';
import { useMusicSessionSubscription } from './useMusicSessionSubscription';
import { useMusicSettingsLoader } from './useMusicSettingsLoader';
import { useNow } from './useNow';
import { VolumeRow } from './VolumeRow';

export interface MusicPlayerProps {
  serverKey: string;
  channelId: string;
  onClose?: () => void;
}

/** Wider than this, the sheet has two columns. */
const TWO_COLUMNS = 720;
const PLAYER_COLUMN_MAX = 440;
const PLAYER_COLUMN_MIN = 320;
const ARTWORK_MAX = 360;
/** Everything in the player column but the artwork: the bars above it, the title, position, transport and volume. */
const PLAYER_CHROME = 460;
const EMPTY_ARTWORK = 160;

const NEXT_REPEAT: Record<MusicRepeat, MusicRepeat> = {
  [MusicRepeat.Off]: MusicRepeat.Queue,
  [MusicRepeat.Queue]: MusicRepeat.Track,
  [MusicRepeat.Track]: MusicRepeat.Off,
};

export function MusicPlayer({ serverKey, channelId, onClose }: MusicPlayerProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const window = useWindowDimensions();
  const [size, setSize] = useState({ width: window.width, height: window.height });
  const [searching, setSearching] = useState(false);

  const server = useServer(serverKey);
  const info = useChannelInfo(serverKey, channelId);
  const myVoice = useMyVoice();
  const session = useMusicSession(serverKey, channelId);
  const settings = useMusicSettings(serverKey);
  const { music } = usePrefs();
  useMusicSessionSubscription(serverKey, channelId);
  useMusicSettingsLoader(serverKey);

  const inChannel = myVoice?.serverKey === serverKey && myVoice.channelId === channelId;
  const playing = session?.state === MusicPlayState.Playing;
  const now = useNow(playing);
  const positionMs = session ? positionAt(session, now || (session.anchorMs ?? 0)) : 0;
  const selfUserId = server?.snapshot?.selfUser.id ?? server?.entry.userId;
  const access = musicAccess(info?.access, session, settings, inChannel, selfUserId);
  const canAddMusic = !!info?.access.musicAdd || !!info?.access.musicManage;
  const item = session?.nowPlaying;
  const itemId = item?.itemId;
  const repeat = session?.repeat ?? MusicRepeat.Off;
  const shuffle = session?.shuffle ?? false;
  const channelName = info?.channel.name ?? 'this channel';
  const close = onClose ?? (() => router.back());

  const control = useCallback(
    (action: MusicAction, what: string, positionMs?: number) =>
      report(playback(serverKey, channelId, action, positionMs, itemId), what),
    [serverKey, channelId, itemId],
  );
  const playPause = () => control(playing ? MusicAction.Pause : MusicAction.Resume, 'toggle playback');

  const menu: MenuAction[] = [
    ...(item?.track.uri ? [{ id: 'copy-link', title: 'Copy link', icon: 'copy' as const }] : []),
    // The You tab's "Lower music while people talk", short enough for a menu; ticked when on.
    { id: 'duck', title: 'Lower while people talk', icon: music.duck ? ('check' as const) : ('volume' as const) },
  ];
  const onMenu = (id: string) => {
    if (id === 'duck') setMusicListening({ duck: !music.duck });
    else if (id === 'copy-link' && item) {
      report(
        Clipboard.setStringAsync(item.track.uri).then(() => toast('Copied link')),
        'copy link',
      );
    }
  };

  // Two columns: the player fits the height without scrolling, as wide as its artwork.
  const twoColumns = size.width >= TWO_COLUMNS;
  const columnMax = Math.min(PLAYER_COLUMN_MAX, Math.round(size.width * 0.45));
  const artworkSize = twoColumns
    ? Math.max(PLAYER_COLUMN_MIN - GUTTER * 2, Math.min(ARTWORK_MAX, columnMax - GUTTER * 2, size.height - PLAYER_CHROME))
    : Math.min(ARTWORK_MAX, size.width - GUTTER * 2);
  const playerWidth = twoColumns ? artworkSize + GUTTER * 2 : size.width;
  const bottom = { paddingBottom: insets.bottom + space.xxl };

  const player = item ? (
    <View style={styles.player}>
      <NowPlaying
        key={item.itemId}
        serverKey={serverKey}
        item={item}
        playing={playing}
        positionMs={positionMs}
        seek={access.seek}
        artworkSize={artworkSize}
        menu={menu}
        onMenu={onMenu}
        onSeek={(at) => control(MusicAction.Seek, 'seek', at)}
      />
      <PlayerControls
        playState={session?.state ?? MusicPlayState.Idle}
        repeat={repeat}
        shuffle={shuffle}
        playPauseAccess={access.playPause}
        prevAccess={access.previous}
        nextAccess={access.next}
        shuffleAccess={access.shuffle}
        repeatAccess={access.repeat}
        onTogglePlayPause={playPause}
        onPrevious={() => control(MusicAction.Previous, 'previous track')}
        onNext={() => control(MusicAction.Next, 'next track')}
        onVoteSkip={() => itemId && report(voteSkip(serverKey, channelId, itemId), 'vote to skip')}
        onCycleRepeat={() => report(setMode(serverKey, channelId, NEXT_REPEAT[repeat], shuffle), 'set repeat mode')}
        onToggleShuffle={() => report(setMode(serverKey, channelId, repeat, !shuffle), 'toggle shuffle')}
      />
      <VolumeRow />
    </View>
  ) : (
    <View style={[styles.player, styles.empty]}>
      <Artwork serverKey={serverKey} title={channelName} size={EMPTY_ARTWORK} radius={EMPTY_ARTWORK / 8} />
      <Text variant="headline" style={styles.emptyTitle}>
        Nothing playing
      </Text>
      <Text variant="callout" color="textSecondary" style={styles.centered}>
        {canAddMusic ? 'Search for a song or upload one.' : 'Songs added here play to everyone in the channel.'}
      </Text>
    </View>
  );

  const lists = searching ? (
    <MusicSearch serverKey={serverKey} channelId={channelId} onCancel={() => setSearching(false)} />
  ) : (
    <>
      {canAddMusic && <AddBar serverKey={serverKey} channelId={channelId} onSearch={() => setSearching(true)} />}
      <Uploads serverKey={serverKey} channelId={channelId} />
      {/* With nothing playing or queued, the player's own note says it all. */}
      {(item || (session?.queue.length ?? 0) > 0) && <UpNext serverKey={serverKey} channelId={channelId} />}
      <History serverKey={serverKey} channelId={channelId} />
    </>
  );

  const onLayout = (event: LayoutChangeEvent) => {
    const { width, height } = event.nativeEvent.layout;
    if (width > 0 && height > 0) setSize({ width, height });
  };

  return (
    <View testID="music-player" style={[styles.root, { backgroundColor: colors.elevated }]} onLayout={onLayout}>
      <SheetGrabber />
      <PlayerTopBar
        channelName={channelName}
        onJoin={inChannel ? undefined : () => report(joinVoice(serverKey, channelId), 'join voice')}
        onClose={close}
      />
      {twoColumns ? (
        <View style={styles.columns}>
          <ScrollView style={{ width: playerWidth, flexGrow: 0 }} contentContainerStyle={bottom}>
            {player}
          </ScrollView>
          <ScrollView style={styles.fill} contentContainerStyle={bottom} keyboardShouldPersistTaps="handled">
            {searching && <View style={styles.searchTop} />}
            {lists}
          </ScrollView>
        </View>
      ) : searching ? (
        <>
          {item && (
            <MiniPlayer
              serverKey={serverKey}
              item={item}
              playing={playing}
              canPlayPause={access.playPause.allowed}
              onPlayPause={playPause}
              onOpen={() => setSearching(false)}
            />
          )}
          <ScrollView style={styles.fill} contentContainerStyle={[styles.searchTop, bottom]} keyboardShouldPersistTaps="handled">
            {lists}
          </ScrollView>
        </>
      ) : (
        <ScrollView style={styles.fill} contentContainerStyle={bottom} keyboardShouldPersistTaps="handled">
          {player}
          {lists}
        </ScrollView>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
  fill: { flex: 1 },
  columns: { flex: 1, flexDirection: 'row' },
  player: { paddingHorizontal: GUTTER },
  empty: { alignItems: 'center', paddingTop: space.xxl, gap: space.xs },
  emptyTitle: { marginTop: space.lg },
  centered: { textAlign: 'center' },
  searchTop: { paddingTop: space.md },
});
