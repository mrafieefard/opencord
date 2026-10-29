// Where a search looks (spec §8.7): one chip per source the server offers,
// the chosen one filled. Hidden with a single source.
import { ScrollView, StyleSheet } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Text, space, useTheme } from '@/ui';

import { GUTTER } from './TrackRow';

export interface SourceChipsProps {
  sources: string[];
  selected: string;
  onSelect: (source: string) => void;
}

const CHIP_HEIGHT = 32;

export function sourceLabel(source: string): string {
  if (source === 'youtube') return 'YouTube';
  if (source === 'soundcloud') return 'SoundCloud';
  if (source === 'local') return 'Local';
  return source.charAt(0).toUpperCase() + source.slice(1);
}

export function SourceChips({ sources, selected, onSelect }: SourceChipsProps) {
  const { colors } = useTheme();
  if (sources.length < 2) return null;
  return (
    <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.row} keyboardShouldPersistTaps="handled">
      {sources.map((source) => {
        const on = source === selected;
        return (
          <Touchable
            key={source}
            onPress={() => onSelect(source)}
            accessible
            accessibilityRole="radio"
            accessibilityState={{ selected: on }}
            accessibilityLabel={sourceLabel(source)}
            activeScale={0.96}
            hitSlop={{ top: 6, bottom: 6 }}
            style={[
              styles.chip,
              { backgroundColor: on ? colors.accent : colors.selected },
            ]}
          >
            <Text variant="footnote" color={on ? 'onAccent' : 'textSecondary'} style={styles.label}>
              {sourceLabel(source)}
            </Text>
          </Touchable>
        );
      })}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  row: { gap: space.sm, paddingHorizontal: GUTTER, paddingVertical: space.sm },
  chip: {
    height: CHIP_HEIGHT,
    borderRadius: CHIP_HEIGHT / 2,
    paddingHorizontal: space.md + 2,
    justifyContent: 'center',
  },
  label: { includeFontPadding: false },
});
