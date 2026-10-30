// What stands in the composer's field while recording (media uploads design
// §11): a pulsing red dot, the time, the live waveform in the voice blue
// (one bar per level, the newest at the right), and "‹ Slide to cancel",
// which turns red as "Release to cancel" once far enough. Locked, it has
// Stop; stopped, it plays the draft back.
import type { VoiceDraftRecord } from '@opencord/core-native';
import { useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withRepeat, withTiming } from 'react-native-reanimated';

import { clock } from '@/features/chat/attachments/VoiceMessageAttachment';
import { draftItem, pauseVoice, playVoice, resumeVoice, seekVoice } from '@/state/actions/voiceMessages';
import { useAppStore } from '@/state/store';
import { GlassSurface, Icon, Text, space, useTheme } from '@/ui';

import type { RecorderMode } from './useVoiceRecorder';
import { Waveform } from './Waveform';

export interface RecorderPillProps {
  mode: Exclude<RecorderMode, 'idle'>;
  cancelArmed: boolean;
  draft: VoiceDraftRecord | null;
  onStop: () => void;
  height: number;
}

export function RecorderPill({ mode, cancelArmed, draft, onStop, height }: RecorderPillProps) {
  return (
    <GlassSurface style={[styles.pill, { minHeight: height, borderRadius: height / 2 }]} testID="voice-recorder">
      {mode === 'draft' ? <DraftPreview draft={draft} /> : <Live mode={mode} cancelArmed={cancelArmed} onStop={onStop} />}
    </GlassSurface>
  );
}

function Live({ mode, cancelArmed, onStop }: { mode: 'holding' | 'locked'; cancelArmed: boolean; onStop: () => void }) {
  const { colors } = useTheme();
  const recording = useAppStore((state) => state.voiceRecording);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(timer);
  }, []);
  const pulse = useSharedValue(1);
  useEffect(() => {
    pulse.set(withRepeat(withTiming(0.25, { duration: 600 }), -1, true));
  }, [pulse]);
  const dot = useAnimatedStyle(() => ({ opacity: pulse.get() }));
  const elapsed = recording ? now - recording.startedAtMs : 0;

  return (
    <>
      <Animated.View style={[styles.dot, { backgroundColor: colors.recording }, dot]} />
      <Text variant="body" tabular style={styles.time}>
        {clock(elapsed)}
      </Text>
      {mode === 'holding' && cancelArmed ? (
        <Text variant="callout" color="recording" style={styles.hint} numberOfLines={1}>
          Release to cancel
        </Text>
      ) : (
        <View style={styles.wave}>
          <Waveform levels={recording?.levels ?? []} unit={1} alignEnd color={colors.voice} height={24} />
        </View>
      )}
      {mode === 'holding' && !cancelArmed && (
        <Text variant="callout" color="textSecondary" numberOfLines={1}>
          ‹ Slide to cancel
        </Text>
      )}
      {mode === 'locked' && (
        <Touchable onPress={onStop} hitSlop={8}>
          <View style={styles.stop} accessible accessibilityRole="button" accessibilityLabel="Stop recording">
            <Icon name="stop" size={18} color="recording" />
          </View>
        </Touchable>
      )}
    </>
  );
}

function DraftPreview({ draft }: { draft: VoiceDraftRecord | null }) {
  const { colors } = useTheme();
  const key = draft ? `draft/${draft.draftId}` : '';
  const playback = useAppStore((state) => (state.voicePlayback?.key === key ? state.voicePlayback : null));
  if (!draft) return <View style={styles.wave} />;
  const durationMs = playback?.durationMs || draft.durationMs;
  const positionMs = playback?.positionMs ?? 0;
  const playing = playback?.playing ?? false;
  const toggle = () => {
    if (playing) pauseVoice();
    else if (playback && positionMs > 0) resumeVoice();
    else void playVoice(draftItem(draft));
  };
  const seek = (fraction: number) => {
    if (playback) seekVoice(fraction * durationMs);
    else void playVoice(draftItem(draft), fraction * durationMs);
  };
  return (
    <>
      <Touchable onPress={toggle} hitSlop={8}>
        <View style={styles.stop} accessible accessibilityRole="button" accessibilityLabel={playing ? 'Pause' : 'Play recording'}>
          <Icon name={playing ? 'pause' : 'play'} size={18} color="voice" />
        </View>
      </Touchable>
      <View style={styles.wave}>
        <Waveform levels={draft.waveform} progress={durationMs > 0 ? positionMs / durationMs : 0} onSeek={seek} height={24} />
      </View>
      <Text variant="callout" color="textSecondary" tabular style={{ color: colors.textSecondary }}>
        {clock(playing || positionMs > 0 ? positionMs : durationMs)}
      </Text>
    </>
  );
}

const DOT = 10;

const styles = StyleSheet.create({
  pill: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingHorizontal: space.md + 2,
  },
  dot: { width: DOT, height: DOT, borderRadius: DOT / 2 },
  time: { minWidth: 40 },
  wave: { flex: 1 },
  hint: { flex: 1, textAlign: 'center' },
  stop: { width: 28, height: 28, alignItems: 'center', justifyContent: 'center' },
});
