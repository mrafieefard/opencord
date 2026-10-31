// One track in a list (spec §8.7): the queue, search results and recently
// played share it. 64 pt: the artwork, with who added it as a small avatar
// on its corner; the title over the artist; then the length, or the row's
// own button.
import type { MusicTrackRecord } from '@opencord/core-native';
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { useImageFile } from '@/features/servers/useImageFile';
import { useMember } from '@/state/selectors';
import { Avatar, Row, Text, radius, space, useTheme, weightFamily } from '@/ui';

import { Artwork } from './Artwork';
import { formatDuration } from './position';

export const TRACK_ROW_HEIGHT = 64;
const ARTWORK = 48;
const BADGE = 20;
const BADGE_RING = 2;
/** The rows' side margin, the player's too. */
export const GUTTER = space.xl;

export interface TrackRowProps {
  serverKey: string;
  track: MusicTrackRecord;
  /** Shown on the artwork's corner. */
  requesterId?: string;
  /** In place of the length: the row's own button. */
  trailing?: ReactNode;
  onPress?: () => void;
  onLongPress?: () => void;
  accessibilityLabel: string;
}

/** An uploaded song, which the server plays itself. */
export function isLocalTrack(track: MusicTrackRecord): boolean {
  return track.localSongId !== undefined || track.source === 'local';
}

export function trackLength(track: MusicTrackRecord): string {
  return track.isStream ? 'Live' : formatDuration(track.durationMs);
}

function RequesterBadge({ serverKey, userId }: { serverKey: string; userId: string }) {
  const { colors } = useTheme();
  const member = useMember(serverKey, userId);
  const file = useImageFile(serverKey, member?.avatarHash);
  if (!member) return null;
  return (
    <View style={[styles.badge, { backgroundColor: colors.elevated }]}>
      <Avatar name={member.name} file={file} size={BADGE} />
    </View>
  );
}

export function TrackRow({ serverKey, track, requesterId, trailing, onPress, onLongPress, accessibilityLabel }: TrackRowProps) {
  const { colors } = useTheme();
  return (
    <Row
      height={TRACK_ROW_HEIGHT}
      onPress={onPress}
      onLongPress={onLongPress}
      accessibilityLabel={accessibilityLabel}
      style={[styles.row, { backgroundColor: colors.elevated }]}
    >
      <View style={styles.artwork}>
        <Artwork serverKey={serverKey} artworkId={track.artworkId} title={track.title} size={ARTWORK} radius={radius.row} />
        {requesterId ? <RequesterBadge serverKey={serverKey} userId={requesterId} /> : null}
      </View>
      <View style={styles.text}>
        <View style={styles.titleLine}>
          <Text variant="callout" numberOfLines={1} style={styles.title}>
            {track.title}
          </Text>
          {isLocalTrack(track) ? (
            <View style={[styles.pill, { backgroundColor: colors.selected }]}>
              <Text variant="footnote" color="textSecondary" style={styles.pillText}>
                Local
              </Text>
            </View>
          ) : null}
        </View>
        <Text variant="footnote" color="textSecondary" numberOfLines={1}>
          {trailing ? `${track.artist} · ${trackLength(track)}` : track.artist}
        </Text>
      </View>
      {trailing ?? (
        <Text variant="footnote" color="textMuted" tabular>
          {trackLength(track)}
        </Text>
      )}
    </Row>
  );
}

const styles = StyleSheet.create({
  row: { paddingHorizontal: GUTTER },
  artwork: { width: ARTWORK, height: ARTWORK, marginRight: space.md },
  badge: {
    position: 'absolute',
    right: -BADGE_RING * 3,
    bottom: -BADGE_RING * 3,
    borderRadius: BADGE,
    padding: BADGE_RING / 2,
  },
  text: { flex: 1, marginRight: space.md, gap: 2 },
  titleLine: { flexDirection: 'row', alignItems: 'center', gap: space.sm },
  title: { flexShrink: 1, fontFamily: weightFamily(500) },
  pill: { borderRadius: radius.pill, paddingHorizontal: space.xs + 2 },
  pillText: { fontSize: 11, lineHeight: 16 },
});
