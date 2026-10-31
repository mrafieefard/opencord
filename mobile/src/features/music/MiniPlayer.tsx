// What plays, small (spec §8.7): above search on a phone, where the big
// player would push the results off the screen. A tap goes back to it.
import type { MusicQueueItemRecord } from '@opencord/core-native';
import { StyleSheet, View } from 'react-native';

import { IconButton, Row, Text, space, useTheme } from '@/ui';

import { Artwork } from './Artwork';
import { GUTTER } from './TrackRow';

export interface MiniPlayerProps {
  serverKey: string;
  item: MusicQueueItemRecord;
  playing: boolean;
  canPlayPause: boolean;
  onPlayPause: () => void;
  onOpen: () => void;
}

const HEIGHT = 60;
const ARTWORK = 40;

export function MiniPlayer({ serverKey, item, playing, canPlayPause, onPlayPause, onOpen }: MiniPlayerProps) {
  const { colors } = useTheme();
  return (
    <Row
      height={HEIGHT}
      onPress={onOpen}
      accessibilityLabel={`Now playing: ${item.track.title}, ${item.track.artist}. Back to the player`}
      style={[styles.row, { borderBottomColor: colors.border }]}
    >
      <Artwork serverKey={serverKey} artworkId={item.track.artworkId} title={item.track.title} size={ARTWORK} radius={6} />
      <View style={styles.text}>
        <Text variant="subhead" numberOfLines={1}>
          {item.track.title}
        </Text>
        <Text variant="footnote" color="textSecondary" numberOfLines={1}>
          {item.track.artist}
        </Text>
      </View>
      {canPlayPause && <IconButton icon={playing ? 'pause' : 'play'} label={playing ? 'Pause' : 'Play'} size={40} onPress={onPlayPause} />}
    </Row>
  );
}

const styles = StyleSheet.create({
  row: { paddingHorizontal: GUTTER, borderBottomWidth: StyleSheet.hairlineWidth },
  text: { flex: 1, marginHorizontal: space.md, gap: 1 },
});
