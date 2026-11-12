// A command's options above the field: one chip each, scrolling sideways
// when there are many. Tapping one edits it; required ones are marked until
// they have a value.
import { ScrollView, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import type { CommandOptionRecord } from '@opencord/core-native';

import { Text, space, useTheme } from '@/ui';

import type { OptionValue } from './useCommandDraft';

export interface CommandChipsProps {
  options: readonly CommandOptionRecord[];
  values: Record<string, OptionValue>;
  /** The option the field edits. */
  active: string | undefined;
  onPress: (name: string) => void;
}

export function CommandChips({ options, values, active, onPress }: CommandChipsProps) {
  const { colors } = useTheme();
  return (
    <ScrollView
      testID="command-chips"
      horizontal
      showsHorizontalScrollIndicator={false}
      keyboardShouldPersistTaps="always"
      contentContainerStyle={styles.content}
    >
      {options.map((option) => {
        const value = values[option.name];
        return (
          <Touchable key={option.name} onPress={() => onPress(option.name)}>
            <View
              testID={`command-option-${option.name}`}
              accessible
              accessibilityRole="button"
              accessibilityLabel={option.name}
              accessibilityState={{ selected: option.name === active }}
              style={[
                styles.chip,
                {
                  backgroundColor: option.name === active ? colors.selected : colors.surface,
                  borderColor: colors.border,
                },
              ]}
            >
              {option.required && !value && (
                <View testID={`command-required-${option.name}`} style={[styles.required, { backgroundColor: colors.recording }]} />
              )}
              <Text variant="footnote" color="textSecondary" numberOfLines={1}>
                {option.name}
              </Text>
              {value && (
                <Text variant="footnote" numberOfLines={1} style={styles.value}>
                  {`: ${value.label}`}
                </Text>
              )}
            </View>
          </Touchable>
        );
      })}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { flexDirection: 'row', gap: 6, paddingHorizontal: 7, paddingVertical: 3 },
  chip: {
    height: 32,
    maxWidth: 220,
    borderRadius: 16,
    borderWidth: StyleSheet.hairlineWidth,
    paddingHorizontal: 11,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
  required: { width: 5, height: 5, borderRadius: 2.5 },
  value: { flexShrink: 1 },
});
