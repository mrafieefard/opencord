// The transport (spec §8.7): shuffle, previous, play or pause, next,
// repeat. Play is a 72 pt circle; the toggles light up with a dot under
// them; repeat-one shows "1". A control the user may not use is dimmed and
// says why when tapped. Without Control music, next votes to skip, with
// the count on its corner.
import { MusicPlayState, MusicRepeat } from '@opencord/core-native';
import * as Haptics from 'expo-haptics';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { toast } from '@/state/actions/ui';
import { Icon, type IconName, Text, useTheme } from '@/ui';

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

const PLAY = 72;
const SKIP = 56;
const TOGGLE = 44;
const DOT = 4;
const DIMMED = 0.35;

interface ControlProps {
  icon: IconName;
  label: string;
  access: ControlAccess;
  size: number;
  glyph: number;
  onPress: () => void;
  /** A toggle: lit when on, with a dot. */
  on?: boolean;
  /** Read out after the label: what a toggle is set to. */
  value?: string;
  badge?: string;
}

function Control({ icon, label, access, size, glyph, onPress, on, value, badge }: ControlProps) {
  const { colors } = useTheme();
  const press = () => {
    if (!access.allowed) {
      if (access.reason) toast(access.reason);
      return;
    }
    void Haptics.selectionAsync();
    onPress();
  };
  return (
    <Touchable
      onPress={press}
      accessible
      accessibilityRole={on === undefined ? 'button' : 'togglebutton'}
      accessibilityLabel={label}
      accessibilityState={{ disabled: !access.allowed, checked: on }}
      accessibilityValue={value === undefined ? undefined : { text: value }}
      accessibilityHint={access.allowed ? undefined : access.reason}
      activeScale={0.86}
      style={[styles.control, { width: size, height: size, opacity: access.allowed ? 1 : DIMMED }]}
    >
      <Icon name={icon} size={glyph} color={on === false ? 'textMuted' : 'text'} />
      {on !== undefined && <View style={[styles.dot, { backgroundColor: on ? colors.text : 'transparent' }]} />}
      {badge !== undefined && (
        <View style={[styles.badge, { backgroundColor: colors.selected, borderColor: colors.elevated }]}>
          <Text variant="footnote" tabular style={styles.badgeText}>
            {badge}
          </Text>
        </View>
      )}
    </Touchable>
  );
}

export function PlayerControls(props: PlayerControlsProps) {
  const { colors } = useTheme();
  const { playState, repeat, shuffle, playPauseAccess, nextAccess } = props;
  const playing = playState === MusicPlayState.Playing;

  const playPause = () => {
    if (!playPauseAccess.allowed) {
      if (playPauseAccess.reason) toast(playPauseAccess.reason);
      return;
    }
    void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
    props.onTogglePlayPause();
  };

  return (
    <View style={styles.row}>
      <Control icon="shuffle" label="Shuffle" access={props.shuffleAccess} size={TOGGLE} glyph={22} on={shuffle} onPress={props.onToggleShuffle} />
      <Control icon="previous" label="Previous" access={props.prevAccess} size={SKIP} glyph={30} onPress={props.onPrevious} />
      <Touchable
        onPress={playPause}
        accessible
        accessibilityRole="button"
        accessibilityLabel={playing ? 'Pause' : 'Play'}
        accessibilityState={{ disabled: !playPauseAccess.allowed }}
        accessibilityHint={playPauseAccess.allowed ? undefined : playPauseAccess.reason}
        activeScale={0.92}
        style={[styles.play, { backgroundColor: colors.accent, opacity: playPauseAccess.allowed ? 1 : DIMMED }]}
      >
        <Icon name={playing ? 'pause' : 'play'} size={32} color="onAccent" />
      </Touchable>
      <Control
        icon="next"
        label={nextAccess.isVoteSkip ? (nextAccess.voteSkipLabel ?? 'Vote to skip') : 'Next'}
        access={nextAccess}
        size={SKIP}
        glyph={30}
        badge={nextAccess.voteCount}
        onPress={nextAccess.isVoteSkip ? props.onVoteSkip : props.onNext}
      />
      <Control
        icon={repeat === MusicRepeat.Track ? 'repeatOne' : 'repeat'}
        label="Repeat"
        value={repeat === MusicRepeat.Off ? 'Off' : repeat === MusicRepeat.Queue ? 'The queue' : 'This song'}
        access={props.repeatAccess}
        size={TOGGLE}
        glyph={22}
        on={repeat !== MusicRepeat.Off}
        onPress={props.onCycleRepeat}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingVertical: 20 },
  control: { alignItems: 'center', justifyContent: 'center', borderRadius: 999 },
  play: { width: PLAY, height: PLAY, borderRadius: PLAY / 2, alignItems: 'center', justifyContent: 'center' },
  dot: { position: 'absolute', bottom: 2, width: DOT, height: DOT, borderRadius: DOT / 2 },
  badge: {
    position: 'absolute',
    top: 2,
    right: -4,
    borderRadius: 9,
    borderWidth: 2,
    paddingHorizontal: 5,
  },
  badgeText: { fontSize: 11, lineHeight: 14 },
});
