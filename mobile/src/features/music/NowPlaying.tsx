// Artwork, track titles, and seeking slider for the currently playing track (spec §8.7).
import type { MusicQueueItemRecord } from '@opencord/core-native';
import { useCallback, useEffect, useState } from 'react';
import { Image, StyleSheet, View } from 'react-native';

import { getCore } from '@/core';
import { toast } from '@/state/actions/ui';
import { useMember } from '@/state/selectors';
import { Icon, Slider, Text, radius, space, useTheme } from '@/ui';

import type { ControlAccess } from './musicAccess';
import { formatDuration } from './position';

export interface NowPlayingProps {
  serverKey: string;
  nowPlaying?: MusicQueueItemRecord;
  positionMs: number;
  seekAccess: ControlAccess;
  containerWidth: number;
  onSeek: (positionMs: number) => void;
}

export function NowPlaying({
  serverKey,
  nowPlaying,
  positionMs,
  seekAccess,
  containerWidth,
  onSeek,
}: NowPlayingProps) {
  const { colors } = useTheme();
  const track = nowPlaying?.track;
  const requester = useMember(serverKey, nowPlaying?.requesterId ?? '');

  const [fetchedArtwork, setFetchedArtwork] = useState<{ id: string; uri: string | null } | null>(null);
  const [draggingPos, setDraggingPos] = useState<number | null>(null);

  const artworkUri = track?.artworkId && fetchedArtwork?.id === track.artworkId ? fetchedArtwork.uri : null;
  const artworkSize = Math.max(120, Math.min(280, containerWidth - space.lg * 4));

  useEffect(() => {
    const artworkId = track?.artworkId;
    if (!artworkId) return;
    let cancelled = false;
    getCore()
      .musicArtwork(serverKey, artworkId, Math.round(artworkSize))
      .then((uri) => {
        if (!cancelled && uri) setFetchedArtwork({ id: artworkId, uri });
      })
      .catch(() => {
        if (!cancelled) setFetchedArtwork({ id: artworkId, uri: null });
      });
    return () => {
      cancelled = true;
    };
  }, [serverKey, track?.artworkId, artworkSize]);

  const durationMs = track?.durationMs ?? 0;
  const currentPos = draggingPos ?? Math.min(positionMs, durationMs > 0 ? durationMs : positionMs);

  const handleSliderChange = useCallback((value: number) => {
    setDraggingPos(value);
  }, []);

  const handleSliderCommit = useCallback(
    (value: number) => {
      setDraggingPos(null);
      if (seekAccess.allowed) {
        onSeek(Math.round(value));
      } else if (seekAccess.reason) {
        toast(seekAccess.reason);
      }
    },
    [onSeek, seekAccess],
  );

  const handleSliderDisabledPress = useCallback(() => {
    if (!seekAccess.allowed && seekAccess.reason) {
      toast(seekAccess.reason);
    }
  }, [seekAccess]);

  const elapsedText = formatDuration(currentPos);
  const remainingText = track?.isStream
    ? 'LIVE'
    : durationMs > 0
      ? `-${formatDuration(Math.max(0, durationMs - currentPos))}`
      : '0:00';

  const subtitle = track ? track.artist : '';
  const requesterName = requester?.name ?? 'Member';
  const sourceName = track?.sourceDisplayName ?? track?.source ?? '';
  const metaText = track
    ? `Added by ${requesterName}${sourceName ? ` · ${sourceName}` : ''}`
    : '';

  return (
    <View style={styles.container}>
      {/* Artwork */}
      <View
        style={[
          styles.artworkBox,
          {
            width: artworkSize,
            height: artworkSize,
            backgroundColor: colors.surface,
            borderRadius: radius.image,
          },
        ]}
      >
        {artworkUri ? (
          <Image
            source={{ uri: artworkUri }}
            style={[styles.artworkImage, { borderRadius: radius.image }]}
            resizeMode="cover"
            accessibilityLabel={`${track?.title ?? 'Music'} artwork`}
          />
        ) : (
          <Icon name="music" size={64} color="textMuted" />
        )}
      </View>

      {/* Track titles */}
      <View style={styles.titleBox}>
        <Text variant="title2" style={styles.title} numberOfLines={2}>
          {track ? track.title : 'Nothing playing'}
        </Text>
        {subtitle ? (
          <Text variant="callout" color="textSecondary" numberOfLines={1}>
            {subtitle}
          </Text>
        ) : null}
        {metaText ? (
          <Text variant="footnote" color="textMuted" numberOfLines={1}>
            {metaText}
          </Text>
        ) : null}
      </View>

      {/* Progress slider and timestamps */}
      <View style={styles.progressContainer} onTouchStart={handleSliderDisabledPress}>
        <Slider
          value={currentPos}
          min={0}
          max={durationMs > 0 ? durationMs : Math.max(1, currentPos)}
          step={1000}
          accent="voice"
          disabled={!seekAccess.allowed}
          onChange={handleSliderChange}
          onCommit={handleSliderCommit}
          accessibilityLabel="Progress"
        />
        <View style={styles.timeRow}>
          <Text variant="caption" color="textMuted">
            {elapsedText}
          </Text>
          <Text variant="caption" color="textMuted">
            {remainingText}
          </Text>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    alignItems: 'center',
    width: '100%',
    paddingHorizontal: space.lg,
    paddingTop: space.sm,
  },
  artworkBox: {
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: space.md,
    overflow: 'hidden',
  },
  artworkImage: {
    width: '100%',
    height: '100%',
  },
  titleBox: {
    width: '100%',
    alignItems: 'center',
    marginBottom: space.sm,
    gap: 2,
  },
  title: {
    textAlign: 'center',
  },
  progressContainer: {
    width: '100%',
    marginTop: space.xs,
  },
  timeRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    width: '100%',
    marginTop: space.xs,
  },
});
