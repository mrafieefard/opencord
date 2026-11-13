// A reply only this device sees (design §26.3): "Only you can see this",
// and a way to dismiss it. Never stored.
import { memo } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { CommandRunStateRecord } from '@opencord/core-native';

import { dismissEphemeral } from '@/state/actions/commands';
import type { EphemeralNote as Note } from '@/state/types';
import { Icon, Text, space, useTheme } from '@/ui';

import { ThinkingDots } from './ThinkingDots';

export interface EphemeralNoteProps {
  serverKey: string;
  channelId: string;
  note: Note;
}

export const EphemeralNote = memo(function EphemeralNote({ serverKey, channelId, note }: EphemeralNoteProps) {
  const { colors } = useTheme();
  const settled = note.state === CommandRunStateRecord.Answered;
  return (
    <View style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]} testID="ephemeral-note">
      <View style={styles.line}>
        {note.state === CommandRunStateRecord.NoAnswer && <Icon name="clock" size={14} color="textMuted" />}
        <Text variant="callout" color={note.error ? 'recording' : settled ? 'text' : 'textMuted'} style={styles.text}>
          {note.content}
        </Text>
        {note.state === CommandRunStateRecord.Thinking && <ThinkingDots />}
      </View>
      <View style={styles.line}>
        <Text variant="footnote" color="textMuted">
          {'Only you can see this · '}
        </Text>
        <Touchable onPress={() => dismissEphemeral(serverKey, channelId, note)}>
          <View accessible accessibilityRole="button" accessibilityLabel="Dismiss">
            <Text variant="footnote" color="textSecondary" style={styles.dismiss}>
              Dismiss
            </Text>
          </View>
        </Touchable>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  card: {
    marginHorizontal: space.md,
    marginBottom: space.sm,
    paddingHorizontal: 11,
    paddingVertical: 7,
    borderRadius: 12,
    borderWidth: StyleSheet.hairlineWidth,
    gap: 2,
  },
  line: { flexDirection: 'row', alignItems: 'center', gap: space.xs },
  text: { flexShrink: 1 },
  dismiss: { textDecorationLine: 'underline' },
});
