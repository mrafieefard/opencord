// The list's row of sources: Opencord, then each plugin's icon. Tapping one
// goes to its commands; scrolling the list moves the highlight.
import { useEffect, useRef, type ComponentRef } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useReducedMotion, useSharedValue, withTiming } from 'react-native-reanimated';

import { useTheme } from '@/ui';

import type { CommandGroup } from './commandRows';
import { SourceIcon } from './SourceIcon';

export const SOURCE_ROW_HEIGHT = 55;

const BUTTON = 40;
const GAP = 4;
const PAD = 8;
const STEP = BUTTON + GAP;
const MOVE_MS = 150;

export interface SourceRowProps {
  serverKey: string;
  groups: readonly CommandGroup[];
  selected: number;
  onSelect: (group: number) => void;
}

export function SourceRow({ serverKey, groups, selected, onSelect }: SourceRowProps) {
  const { colors } = useTheme();
  const reduced = useReducedMotion();
  const scroll = useRef<ComponentRef<typeof ScrollView>>(null);
  const left = useSharedValue(selected * STEP);

  useEffect(() => {
    const target = selected * STEP;
    left.set(reduced ? target : withTiming(target, { duration: MOVE_MS }));
    scroll.current?.scrollTo({ x: Math.max(0, target - STEP), animated: !reduced });
  }, [selected, reduced, left]);

  const highlight = useAnimatedStyle(() => ({ transform: [{ translateX: left.get() }] }));

  return (
    <View style={[styles.row, { borderBottomColor: colors.border }]}>
      <ScrollView
        ref={scroll}
        horizontal
        showsHorizontalScrollIndicator={false}
        keyboardShouldPersistTaps="always"
        contentContainerStyle={styles.content}
      >
        <Animated.View style={[styles.highlight, { backgroundColor: colors.selected }, highlight]} />
        {groups.map((group, index) => (
          <Touchable key={group.source.id} onPress={() => onSelect(index)} style={styles.button}>
            <View
              accessible
              accessibilityRole="button"
              accessibilityLabel={group.source.name}
              accessibilityState={{ selected: index === selected }}
              testID={`command-source-${index}`}
            >
              <SourceIcon serverKey={serverKey} source={group.source} size={30} radius={9} />
            </View>
          </Touchable>
        ))}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  row: { height: SOURCE_ROW_HEIGHT, borderBottomWidth: StyleSheet.hairlineWidth },
  content: { paddingHorizontal: PAD, paddingVertical: 7, gap: GAP },
  highlight: { position: 'absolute', top: 7, left: PAD, width: BUTTON, height: BUTTON, borderRadius: 12 },
  button: { width: BUTTON, height: BUTTON, alignItems: 'center', justifyContent: 'center' },
});
