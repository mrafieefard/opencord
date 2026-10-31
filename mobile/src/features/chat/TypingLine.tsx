import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { Text, space } from '@/ui';

export function formatTyping(names: string[]): string | null {
  if (names.length === 0) return null;
  if (names.length === 1) return `${names[0]} is typing…`;
  if (names.length === 2) return `${names[0]} and ${names[1]} are typing…`;
  return 'Several people are typing…';
}

export interface TypingLineProps {
  names: string[];
}

export const TypingLine = memo(function TypingLine({ names }: TypingLineProps) {
  const text = formatTyping(names);
  if (!text) return null;

  return (
    <View style={styles.container} testID="typing-line">
      <Text variant="footnote" color="textMuted" numberOfLines={1}>
        {text}
      </Text>
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    height: 20,
    paddingHorizontal: space.lg,
    justifyContent: 'center',
  },
});
