// How loud the music is for this user (spec §8.7): the speaker mutes, the
// bar sets 0–200 % and settles on 100 % with a tick. Dragging unmutes.
import { StyleSheet, View } from 'react-native';

import { setMusicListening } from '@/state/actions/prefs';
import { usePrefs } from '@/state/selectors';
import { IconButton, Scrubber, Text, space } from '@/ui';

const STEP = 0.01;
const PERCENT_WIDTH = 48;

export function VolumeRow() {
  const { music } = usePrefs();
  const silent = music.muted || music.volume === 0;
  const percent = `${Math.round(music.volume * 100)} %`;
  return (
    <View style={styles.row}>
      <IconButton
        icon={silent ? 'volumeOff' : 'volume'}
        label={music.muted ? 'Unmute music' : 'Mute music'}
        size={36}
        onPress={() => setMusicListening({ muted: !music.muted })}
      />
      <View style={[styles.bar, music.muted && styles.muted]}>
        <Scrubber
          testID="music-volume"
          value={music.volume}
          max={2}
          step={STEP}
          accessibilityStep={0.05}
          detent={1}
          onChange={(volume) => setMusicListening(music.muted ? { volume, muted: false } : { volume })}
          accessibilityLabel="Music volume"
          accessibilityValue={music.muted ? `${percent}, muted` : percent}
        />
      </View>
      <Text variant="footnote" color="textMuted" tabular style={styles.percent}>
        {percent}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: space.sm },
  bar: { flex: 1 },
  muted: { opacity: 0.4 },
  percent: { width: PERCENT_WIDTH, textAlign: 'right' },
});
