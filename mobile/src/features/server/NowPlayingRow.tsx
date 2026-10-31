import { memo } from 'react';
import { StyleSheet, View } from 'react-native';
import type { MusicNowPlayingRecord } from '@opencord/core-native';

import { Icon, Text, space } from '@/ui';

export interface NowPlayingRowProps {
  nowPlaying: MusicNowPlayingRecord;
}

export const NowPlayingRow = memo(function NowPlayingRow({ nowPlaying }: NowPlayingRowProps) {
  const trackText = nowPlaying.artist ? `${nowPlaying.title} — ${nowPlaying.artist}` : nowPlaying.title;

  return (
    <View style={styles.row} accessibilityLabel={`Now playing: ${trackText}`}>
      <View style={styles.icon}>
        <Icon name="music" size={14} color="textSecondary" />
      </View>
      <Text variant="caption" color="textSecondary" numberOfLines={1} style={styles.text}>
        {trackText}
      </Text>
    </View>
  );
});

const styles = StyleSheet.create({
  row: {
    height: 28,
    flexDirection: 'row',
    alignItems: 'center',
    paddingLeft: space.xl + space.md,
    paddingRight: space.lg,
  },
  icon: {
    marginRight: space.xs,
  },
  text: {
    flex: 1,
  },
});
