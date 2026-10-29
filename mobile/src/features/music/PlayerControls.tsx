// 5-button transport controls for the music player: Shuffle, Prev, Play/Pause, Next, Repeat (spec §8.7).
import { MusicPlayState, MusicRepeat } from '@opencord/core-native';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { toast } from '@/state/actions/ui';
import { Button, Icon, space, useTheme } from '@/ui';

import type { ControlAccess } from './musicAccess';

export interface PlayerControlsProps {
  playState: MusicPlayState;
  repeat: MusicRepeat;
  shuffle: boolean;
  playPauseAccess: ControlAccess;
  prevAccess: ControlAccess;
  nextAccess: ControlAccess;
  shuffleAccess: ControlAccess;
  repeatAccess: ControlAccess;
  onTogglePlayPause: () => void;
  onPrevious: () => void;
  onNext: () => void;
  onVoteSkip: () => void;
  onCycleRepeat: () => void;
  onToggleShuffle: () => void;
}

const PLAY_BUTTON_SIZE = 48;
const ICON_BUTTON_SIZE = 44;

export function PlayerControls({
  playState,
  repeat,
  shuffle,
  playPauseAccess,
  prevAccess,
  nextAccess,
  shuffleAccess,
  repeatAccess,
  onTogglePlayPause,
  onPrevious,
  onNext,
  onVoteSkip,
  onCycleRepeat,
  onToggleShuffle,
}: PlayerControlsProps) {
  const { colors } = useTheme();
  const isPlaying = playState === MusicPlayState.Playing;

  const handleShufflePress = () => {
    if (shuffleAccess.allowed) {
      onToggleShuffle();
    } else if (shuffleAccess.reason) {
      toast(shuffleAccess.reason);
    }
  };

  const handlePrevPress = () => {
    if (prevAccess.allowed) {
      onPrevious();
    } else if (prevAccess.reason) {
      toast(prevAccess.reason);
    }
  };

  const handlePlayPausePress = () => {
    if (playPauseAccess.allowed) {
      onTogglePlayPause();
    } else if (playPauseAccess.reason) {
      toast(playPauseAccess.reason);
    }
  };

  const handleNextPress = () => {
    if (nextAccess.isVoteSkip) {
      if (nextAccess.allowed) {
        onVoteSkip();
      } else if (nextAccess.reason) {
        toast(nextAccess.reason);
      }
    } else {
      if (nextAccess.allowed) {
        onNext();
      } else if (nextAccess.reason) {
        toast(nextAccess.reason);
      }
    }
  };

  const handleRepeatPress = () => {
    if (repeatAccess.allowed) {
      onCycleRepeat();
    } else if (repeatAccess.reason) {
      toast(repeatAccess.reason);
    }
  };

  const repeatActive = repeat !== MusicRepeat.Off;
  const repeatIconName = repeat === MusicRepeat.Track ? 'repeatOne' : 'repeat';

  return (
    <View style={styles.controlsRow}>
      {/* Shuffle */}
      <Touchable
        onPress={handleShufflePress}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Shuffle"
        style={[styles.smallButton, { opacity: shuffleAccess.allowed ? 1 : 0.4 }]}
      >
        <Icon
          name="shuffle"
          size={22}
          color={shuffle ? 'text' : 'textMuted'}
        />
      </Touchable>

      {/* Previous */}
      <Touchable
        onPress={handlePrevPress}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Previous"
        style={[styles.smallButton, { opacity: prevAccess.allowed ? 1 : 0.4 }]}
      >
        <Icon name="previous" size={26} color="text" />
      </Touchable>

      {/* Play / Pause 64 pt accent circle */}
      <Touchable
        onPress={handlePlayPausePress}
        accessible
        accessibilityRole="button"
        accessibilityLabel={isPlaying ? 'Pause' : 'Play'}
        style={{ opacity: playPauseAccess.allowed ? 1 : 0.4 }}
      >
        <View style={[styles.playButton, { backgroundColor: colors.accent }]}>
          <Icon
            name={isPlaying ? 'pause' : 'play'}
            size={24}
            color="onAccent"
          />
        </View>
      </Touchable>

      {/* Next or Vote to skip */}
      {nextAccess.isVoteSkip ? (
        <Button
          title={nextAccess.voteSkipLabel ?? 'Vote to skip'}
          variant="secondary"
          disabled={!nextAccess.allowed}
          onPress={handleNextPress}
        />
      ) : (
        <Touchable
          onPress={handleNextPress}
          accessible
          accessibilityRole="button"
          accessibilityLabel="Next"
          style={[styles.smallButton, { opacity: nextAccess.allowed ? 1 : 0.4 }]}
        >
          <Icon name="next" size={26} color="text" />
        </Touchable>
      )}

      {/* Repeat */}
      <Touchable
        onPress={handleRepeatPress}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Repeat"
        style={[styles.smallButton, { opacity: repeatAccess.allowed ? 1 : 0.4 }]}
      >
        <Icon
          name={repeatIconName}
          size={22}
          color={repeatActive ? 'text' : 'textMuted'}
        />
      </Touchable>
    </View>
  );
}

const styles = StyleSheet.create({
  controlsRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-evenly',
    width: '100%',
    paddingVertical: space.md,
  },
  playButton: {
    width: PLAY_BUTTON_SIZE,
    height: PLAY_BUTTON_SIZE,
    borderRadius: PLAY_BUTTON_SIZE / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  smallButton: {
    width: ICON_BUTTON_SIZE,
    height: ICON_BUTTON_SIZE,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
