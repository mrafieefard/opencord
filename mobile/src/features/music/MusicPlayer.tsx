// The music player sheet (spec §8.7): artwork, track info, progress slider,
// transport controls, volume & ducking, and segmented sections.
import {
  MusicAction,
  MusicPlayState,
  MusicRepeat,
} from '@opencord/core-native';
import { router } from 'expo-router';
import { useCallback, useState, useSyncExternalStore } from 'react';
import {
  type LayoutChangeEvent,
  ScrollView,
  StyleSheet,
  View,
  useWindowDimensions,
} from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';
import {
  playback,
  setMode,
  voteSkip,
} from '@/state/actions/music';
import { setMusicListening } from '@/state/actions/prefs';
import { joinVoice } from '@/state/actions/voice';
import {
  useChannelInfo,
  useMusicSession,
  useMusicSettings,
  useMyVoice,
  usePrefs,
  useServer,
} from '@/state/selectors';
import {
  Button,
  Icon,
  Segmented,
  SheetHeader,
  Slider,
  Switch,
  Text,
  space,
  useTheme,
} from '@/ui';

import { History } from './History';
import { MusicSearch } from './MusicSearch';
import { NowPlaying } from './NowPlaying';
import { PlayerControls } from './PlayerControls';
import { UpNext } from './UpNext';
import { UploadSection } from './UploadSection';
import { musicAccess } from './musicAccess';
import { positionAt } from './position';
import { useMusicSessionSubscription } from './useMusicSessionSubscription';

export interface MusicPlayerProps {
  serverKey: string;
  channelId: string;
  onClose?: () => void;
}

const SECTION_TABS = ['Up next', 'Search', 'Upload', 'History'];
const VOLUME_STEP = 0.05;

let cachedNow = 0;
const timeListeners = new Set<() => void>();
let timeTimer: ReturnType<typeof setInterval> | null = null;

function subscribeTime(listener: () => void) {
  timeListeners.add(listener);
  if (!timeTimer) {
    cachedNow = Date.now();
    timeTimer = setInterval(() => {
      cachedNow = Date.now();
      timeListeners.forEach((l) => l());
    }, 500);
    timeTimer.unref?.();
  }
  return () => {
    timeListeners.delete(listener);
    if (timeListeners.size === 0 && timeTimer) {
      clearInterval(timeTimer);
      timeTimer = null;
    }
  };
}

function getTimeSnapshot() {
  return cachedNow;
}

export function MusicPlayer({ serverKey, channelId, onClose }: MusicPlayerProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const server = useServer(serverKey);
  const info = useChannelInfo(serverKey, channelId);
  const myVoice = useMyVoice();
  const session = useMusicSession(serverKey, channelId);
  const settings = useMusicSettings(serverKey);
  const { music } = usePrefs();

  // Subscribe to updates while sheet is open
  useMusicSessionSubscription(serverKey, channelId);

  const [containerWidth, setContainerWidth] = useState(360);
  const [selectedSection, setSelectedSection] = useState(0);

  const selfUserId = server?.snapshot?.selfUser.id ?? server?.entry.userId;
  const inChannel =
    myVoice?.serverKey === serverKey && myVoice.channelId === channelId;

  const isPlaying = session?.state === MusicPlayState.Playing;
  const nowMs = useSyncExternalStore(
    isPlaying ? subscribeTime : () => () => {},
    isPlaying ? getTimeSnapshot : () => 0,
    () => 0,
  );
  const currentPositionMs = session ? positionAt(session, nowMs || session.anchorMs) : 0;

  const access = musicAccess(
    info?.access,
    session,
    settings,
    inChannel,
    selfUserId,
  );

  const nowPlaying = session?.nowPlaying;
  const currentItemId = nowPlaying?.itemId;
  const currentSessionState = session?.state;
  const currentRepeat = session?.repeat;
  const currentShuffle = session?.shuffle;
  const channelName = info?.channel.name ?? 'channel';
  const close = onClose ?? (() => router.back());

  const onLayout = useCallback((e: LayoutChangeEvent) => {
    const width = e.nativeEvent.layout.width;
    if (width > 0) setContainerWidth(width);
  }, []);

  const handleJoinVoice = useCallback(() => {
    report(joinVoice(serverKey, channelId), 'join voice');
  }, [serverKey, channelId]);

  const handleTogglePlayPause = useCallback(() => {
    if (!session) return;
    const action =
      currentSessionState === MusicPlayState.Playing
        ? MusicAction.Pause
        : MusicAction.Resume;
    report(
      playback(serverKey, channelId, action, undefined, currentItemId),
      'toggle playback',
    );
  }, [serverKey, channelId, session, currentSessionState, currentItemId]);

  const handlePrevious = useCallback(() => {
    report(
      playback(
        serverKey,
        channelId,
        MusicAction.Previous,
        undefined,
        currentItemId,
      ),
      'previous track',
    );
  }, [serverKey, channelId, currentItemId]);

  const handleNext = useCallback(() => {
    report(
      playback(
        serverKey,
        channelId,
        MusicAction.Next,
        undefined,
        currentItemId,
      ),
      'next track',
    );
  }, [serverKey, channelId, currentItemId]);

  const handleVoteSkip = useCallback(() => {
    if (!currentItemId) return;
    report(
      voteSkip(serverKey, channelId, currentItemId),
      'vote to skip',
    );
  }, [serverKey, channelId, currentItemId]);

  const handleCycleRepeat = useCallback(() => {
    if (!session) return;
    let nextRepeat = MusicRepeat.Off;
    if (currentRepeat === MusicRepeat.Off) nextRepeat = MusicRepeat.Queue;
    else if (currentRepeat === MusicRepeat.Queue) nextRepeat = MusicRepeat.Track;
    else if (currentRepeat === MusicRepeat.Track) nextRepeat = MusicRepeat.Off;

    report(
      setMode(serverKey, channelId, nextRepeat, currentShuffle ?? false),
      'set repeat mode',
    );
  }, [serverKey, channelId, session, currentRepeat, currentShuffle]);

  const handleToggleShuffle = useCallback(() => {
    if (!session) return;
    report(
      setMode(serverKey, channelId, currentRepeat ?? MusicRepeat.Off, !currentShuffle),
      'toggle shuffle',
    );
  }, [serverKey, channelId, session, currentRepeat, currentShuffle]);

  const handleSeek = useCallback(
    (targetMs: number) => {
      report(
        playback(
          serverKey,
          channelId,
          MusicAction.Seek,
          targetMs,
          currentItemId,
        ),
        'seek',
      );
    },
    [serverKey, channelId, currentItemId],
  );

  const percentage = `${Math.round(music.volume * 100)} %`;
  const { width: windowWidth } = useWindowDimensions();
  const isTablet = windowWidth >= 768;
  const leftColWidth = isTablet ? Math.min(380, Math.floor(windowWidth * 0.45)) : containerWidth;

  const playerControlsBlock = (
    <>
      {/* Banner when not connected to this voice channel */}
      {!inChannel ? (
        <View
          style={[
            styles.joinBanner,
            { backgroundColor: colors.surface, borderColor: colors.border },
          ]}
        >
          <View style={styles.joinTextCol}>
            <Text variant="subhead">
              Listen by joining {channelName}
            </Text>
          </View>
          <Button
            title="Join"
            variant="voice"
            onPress={handleJoinVoice}
          />
        </View>
      ) : null}

      {/* Artwork, Track metadata and seek slider */}
      <NowPlaying
        serverKey={serverKey}
        nowPlaying={nowPlaying}
        positionMs={currentPositionMs}
        seekAccess={access.seek}
        containerWidth={isTablet ? leftColWidth : containerWidth}
        onSeek={handleSeek}
      />

      {/* 5-button transport controls */}
      <PlayerControls
        playState={session?.state ?? MusicPlayState.Idle}
        repeat={session?.repeat ?? MusicRepeat.Off}
        shuffle={session?.shuffle ?? false}
        playPauseAccess={access.playPause}
        prevAccess={access.previous}
        nextAccess={access.next}
        shuffleAccess={access.shuffle}
        repeatAccess={access.repeat}
        onTogglePlayPause={handleTogglePlayPause}
        onPrevious={handlePrevious}
        onNext={handleNext}
        onVoteSkip={handleVoteSkip}
        onCycleRepeat={handleCycleRepeat}
        onToggleShuffle={handleToggleShuffle}
      />

      {/* Volume & Listening rows (spec §8.7) */}
      <View style={styles.volumeSection}>
        <View style={styles.volumeSliderRow}>
          <Touchable
            onPress={() => setMusicListening({ muted: !music.muted })}
            accessible
            accessibilityRole="button"
            accessibilityLabel={music.muted ? 'Unmute music' : 'Mute music'}
            style={styles.muteButton}
          >
            <Icon
              name={music.muted ? 'volumeOff' : 'volume'}
              size={22}
              color={music.muted ? 'textMuted' : 'text'}
            />
          </Touchable>
          <View style={styles.sliderWrapper}>
            <Slider
              value={music.volume}
              min={0}
              max={2}
              step={VOLUME_STEP}
              onChange={(volume) => setMusicListening({ volume })}
              accessibilityLabel="Music volume"
            />
          </View>
          <Text variant="callout" color="textSecondary" style={styles.volumePercentage}>
            {percentage}
          </Text>
        </View>

        <View style={styles.duckRow}>
          <Text variant="body">Lower music while people talk</Text>
          <Switch
            value={music.duck}
            onChange={(duck) => setMusicListening({ duck })}
            accessibilityLabel="Lower music while people talk"
          />
        </View>
      </View>
    </>
  );

  const queueSectionBlock = (
    <>
      {/* Section Segmented picker */}
      <View style={styles.segmentedContainer}>
        <Segmented
          values={SECTION_TABS}
          index={selectedSection}
          onChange={setSelectedSection}
        />
      </View>

      {/* Section content */}
      <View style={styles.sectionBody}>
        {selectedSection === 0 ? (
          <UpNext serverKey={serverKey} channelId={channelId} />
        ) : selectedSection === 1 ? (
          <MusicSearch serverKey={serverKey} channelId={channelId} />
        ) : selectedSection === 2 ? (
          <UploadSection serverKey={serverKey} channelId={channelId} />
        ) : (
          <History serverKey={serverKey} channelId={channelId} />
        )}
      </View>
    </>
  );

  return (
    <View style={[styles.root, { backgroundColor: colors.elevated }]}>
      <SheetHeader title="Music" onClose={close} />
      {isTablet ? (
        <View style={styles.tabletRow}>
          <ScrollView
            style={[styles.tabletLeft, { width: leftColWidth, borderRightColor: colors.border }]}
            contentContainerStyle={[
              styles.tabletLeftContent,
              { paddingBottom: insets.bottom + space.lg },
            ]}
            keyboardShouldPersistTaps="handled"
          >
            {playerControlsBlock}
          </ScrollView>
          <ScrollView
            style={styles.tabletRight}
            contentContainerStyle={{ paddingBottom: insets.bottom + space.lg }}
            keyboardShouldPersistTaps="handled"
          >
            {queueSectionBlock}
          </ScrollView>
        </View>
      ) : (
        <ScrollView
          style={styles.scroll}
          contentContainerStyle={[
            styles.content,
            { paddingBottom: insets.bottom + space.lg },
          ]}
          keyboardShouldPersistTaps="handled"
          onLayout={onLayout}
        >
          {playerControlsBlock}
          {queueSectionBlock}
        </ScrollView>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  root: {
    flex: 1,
  },
  scroll: {
    flex: 1,
  },
  content: {
    alignItems: 'center',
    paddingHorizontal: space.md,
  },
  joinBanner: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    width: '100%',
    paddingHorizontal: space.md,
    paddingVertical: space.sm,
    borderRadius: space.sm,
    borderWidth: StyleSheet.hairlineWidth,
    marginBottom: space.sm,
  },
  joinTextCol: {
    flex: 1,
    marginRight: space.sm,
  },
  volumeSection: {
    width: '100%',
    paddingHorizontal: space.lg,
    paddingVertical: space.xs,
    gap: space.xs,
  },
  volumeSliderRow: {
    flexDirection: 'row',
    alignItems: 'center',
    width: '100%',
    gap: space.sm,
  },
  muteButton: {
    width: 36,
    height: 36,
    alignItems: 'center',
    justifyContent: 'center',
  },
  sliderWrapper: {
    flex: 1,
  },
  volumePercentage: {
    minWidth: 48,
    textAlign: 'right',
  },
  duckRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    width: '100%',
    minHeight: 44,
  },
  segmentedContainer: {
    width: '100%',
    paddingHorizontal: space.md,
    paddingVertical: space.md,
  },
  sectionBody: {
    width: '100%',
    paddingVertical: space.md,
  },
  tabletRow: {
    flex: 1,
    flexDirection: 'row',
  },
  tabletLeft: {
    borderRightWidth: StyleSheet.hairlineWidth,
  },
  tabletLeftContent: {
    paddingHorizontal: space.md,
    alignItems: 'center',
  },
  tabletRight: {
    flex: 1,
    paddingHorizontal: space.md,
  },
});
