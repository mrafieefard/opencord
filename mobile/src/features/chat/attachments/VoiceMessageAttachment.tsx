// A voice message in a bubble (media uploads design §11): play and pause,
// the waveform with what has played in the voice blue (a tap or a drag
// seeks), the time ("0:12 / 0:45" while it plays, the length at rest) and
// a 1× → 1.5× → 2× chip. 200 pt wide up to 10 s, 320 pt from a minute.
import { memo } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import type { AttachmentRecord } from '@opencord/core-native';

import { Waveform } from '@/features/voiceMessages/Waveform';
import { attachmentItem, nextVoiceSpeed, pauseVoice, playVoice, resumeVoice, seekVoice } from '@/state/actions/voiceMessages';
import { useAppStore } from '@/state/store';
import { Icon, Text, space, useTheme } from '@/ui';

export interface VoiceMessageAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
  maxWidth: number;
}

const BUTTON = 36;
const SHORT_MS = 10_000;
const LONG_MS = 60_000;
const NARROW = 200;
const WIDE = 320;

/** "0:07", "12:30". */
export function clock(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/** Wider for longer: 200 pt up to 10 s, 320 pt from a minute. */
export function voiceWidth(durationMs: number): number {
  const share = Math.min(Math.max((durationMs - SHORT_MS) / (LONG_MS - SHORT_MS), 0), 1);
  return Math.round(NARROW + share * (WIDE - NARROW));
}

export function speedLabel(speed: number): string {
  return `${speed}×`;
}

export const VoiceMessageAttachment = memo(function VoiceMessageAttachment({ serverKey, attachment, maxWidth }: VoiceMessageAttachmentProps) {
  const { colors } = useTheme();
  const key = `attachment/${serverKey}/${attachment.id}`;
  const playback = useAppStore((state) => (state.voicePlayback?.key === key ? state.voicePlayback : null));
  const speed = useAppStore((state) => state.voiceSpeed);

  const durationMs = playback?.durationMs || attachment.durationMs || 0;
  const positionMs = playback?.positionMs ?? 0;
  const playing = playback?.playing ?? false;
  const progress = durationMs > 0 ? positionMs / durationMs : 0;
  const item = attachmentItem(serverKey, attachment.id);

  const toggle = () => {
    if (playing) pauseVoice();
    else if (playback && positionMs > 0) resumeVoice();
    else void playVoice(item);
  };
  const seek = (fraction: number) => {
    const at = fraction * durationMs;
    if (playback) seekVoice(at);
    else void playVoice(item, at);
  };

  return (
    <View style={[styles.voice, { width: Math.min(voiceWidth(durationMs), maxWidth) }]} testID={`voice-message-${attachment.id}`}>
      <Touchable onPress={toggle} hitSlop={4} style={[styles.button, { backgroundColor: colors.voice }]}>
        <View style={styles.center} accessible accessibilityRole="button" accessibilityLabel={playing ? 'Pause' : 'Play voice message'}>
          <Icon name={playing ? 'pause' : 'play'} size={16} color="onAccent" />
        </View>
      </Touchable>
      <View style={styles.body}>
        <Waveform levels={attachment.waveform} progress={progress} onSeek={durationMs > 0 ? seek : undefined} />
        <View style={styles.meta}>
          <Text variant="caption" color="textSecondary" tabular>
            {playback && (playing || positionMs > 0) ? `${clock(positionMs)} / ${clock(durationMs)}` : clock(durationMs)}
          </Text>
          <Touchable onPress={nextVoiceSpeed} hitSlop={8}>
            <View
              style={[styles.chip, { backgroundColor: colors.hover }]}
              accessible
              accessibilityRole="button"
              accessibilityLabel={`Speed ${speedLabel(speed)}`}
            >
              <Text variant="caption" color="textSecondary" tabular>
                {speedLabel(speed)}
              </Text>
            </View>
          </Touchable>
        </View>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  voice: { flexDirection: 'row', alignItems: 'center', gap: space.sm, paddingVertical: space.xs },
  button: { width: BUTTON, height: BUTTON, borderRadius: BUTTON / 2, overflow: 'hidden' },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  body: { flex: 1, gap: 2 },
  meta: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  chip: { borderRadius: 8, paddingHorizontal: 6, paddingVertical: 1 },
});
