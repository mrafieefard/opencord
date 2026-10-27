// Preferences for listening to music in voice channels: volume, mute, and ducking.
// Reused in the music player (Task 18).
import { StyleSheet, View } from 'react-native';

import { setMusicListening } from '@/state/actions/prefs';
import { usePrefs } from '@/state/selectors';
import { Slider, Switch, Text, space } from '@/ui';

export function MusicListeningRows() {
  const { music } = usePrefs();
  const percentage = `${Math.round(music.volume * 100)} %`;

  return (
    <View style={styles.container}>
      <View style={styles.volumeHeader}>
        <Text variant="body">Volume</Text>
        <Text variant="callout" color="textSecondary">
          {percentage}
        </Text>
      </View>
      <View style={styles.sliderRow}>
        <Slider
          value={music.volume}
          min={0}
          max={2}
          step={0.01}
          onChange={(volume) => setMusicListening({ volume })}
          accessibilityLabel="Music volume"
        />
      </View>
      <View style={styles.toggleRow}>
        <Text variant="body">Mute music</Text>
        <Switch
          value={music.muted}
          onChange={(muted) => setMusicListening({ muted })}
          accessibilityLabel="Mute music"
        />
      </View>
      <View style={styles.toggleRow}>
        <Text variant="body">Lower music while people talk</Text>
        <Switch
          value={music.duck}
          onChange={(duck) => setMusicListening({ duck })}
          accessibilityLabel="Lower music while people talk"
        />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    gap: space.xs,
  },
  volumeHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: space.xs,
  },
  sliderRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: space.xs,
  },
  toggleRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: space.xs,
    minHeight: 44,
  },
});
