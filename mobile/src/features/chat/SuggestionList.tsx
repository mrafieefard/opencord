// The autocomplete suggestion list (design §4.6, P2 Task 4): floating
// above the composer, showing members (@), channels (#), or emoji (:).
// Dismissed by tap, back button, or keyboard hide.
import * as Haptics from 'expo-haptics';
import { useCallback, useEffect, useState } from 'react';
import {
  BackHandler,
  Keyboard,
  Platform,
  ScrollView,
  StyleSheet,
  Text as NativeText,
  useWindowDimensions,
  View,
} from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import Animated, { Easing, Keyframe, useReducedMotion } from 'react-native-reanimated';

import { Avatar, Icon, space, Text, useTheme } from '@/ui';
import { listHeight } from './commands/CommandList';
import { useChatDock } from './dock';
import type { Suggestion } from './suggestions';

export const SUGGESTION_ROW_HEIGHT = 48;

function useKeyboardHeight(): number {
  const [height, setHeight] = useState(() => Keyboard.metrics?.()?.height ?? 0);
  useEffect(() => {
    const showing = Platform.OS === 'ios' ? 'keyboardWillShow' : 'keyboardDidShow';
    const shown = Keyboard.addListener(showing, (event) => setHeight(event.endCoordinates.height));
    const hidden = Keyboard.addListener('keyboardDidHide', () => setHeight(0));
    return () => {
      shown.remove();
      hidden.remove();
    };
  }, []);
  return height;
}

const OPEN_MS = 180;
const CLOSE_MS = 120;
const RISE = 8;
const FALL = 6;

const entering = new Keyframe({
  0: { opacity: 0, transform: [{ translateY: RISE }] },
  100: { opacity: 1, transform: [{ translateY: 0 }], easing: Easing.bezier(0.2, 0.8, 0.2, 1) },
}).duration(OPEN_MS);

const exiting = new Keyframe({
  0: { opacity: 1, transform: [{ translateY: 0 }] },
  100: { opacity: 0, transform: [{ translateY: FALL }], easing: Easing.in(Easing.ease) },
}).duration(CLOSE_MS);

export interface SuggestionListProps {
  items: readonly Suggestion[];
  onSelect: (item: Suggestion) => void;
  onDismiss?: () => void;
}

export function SuggestionList({ items, onSelect, onDismiss }: SuggestionListProps) {
  const { colors } = useTheme();
  const reduced = useReducedMotion();

  useEffect(() => {
    if (!onDismiss) return;
    const back = BackHandler.addEventListener('hardwareBackPress', () => {
      onDismiss();
      return true;
    });
    const hidden = Keyboard.addListener('keyboardDidHide', onDismiss);
    return () => {
      back.remove();
      hidden.remove();
    };
  }, [onDismiss]);

  const handlePick = useCallback(
    (item: Suggestion) => {
      void Haptics.selectionAsync();
      onSelect(item);
    },
    [onSelect],
  );

  const window = useWindowDimensions();
  const dock = useChatDock();
  const keyboard = useKeyboardHeight();

  if (items.length === 0) return null;

  const contentHeight = items.length * SUGGESTION_ROW_HEIGHT;
  const room = (dock.bodyHeight ?? window.height) - dock.height - keyboard;
  const height = listHeight(contentHeight, room);

  return (
    <Animated.View
      entering={reduced ? undefined : entering}
      exiting={reduced ? undefined : exiting}
      testID="suggestion-list"
      style={[
        styles.panel,
        { height, backgroundColor: colors.elevated, borderColor: colors.border },
      ]}
    >
      <ScrollView
        keyboardShouldPersistTaps="always"
        showsVerticalScrollIndicator={false}
      >
        {items.map((item, index) => (
          <SuggestionRowView
            key={`${item.kind}-${index}`}
            item={item}
            onPress={handlePick}
          />
        ))}
      </ScrollView>
    </Animated.View>
  );
}

function SuggestionRowView({
  item,
  onPress,
}: {
  item: Suggestion;
  onPress: (item: Suggestion) => void;
}) {
  const { colors } = useTheme();

  switch (item.kind) {
    case 'member': {
      const displayName = item.member.nickname ?? item.member.user.displayName;
      return (
        <Touchable
          onPress={() => onPress(item)}
          style={styles.row}
          testID={`suggestion-member-${item.member.user.id}`}
        >
          <View
            style={styles.rowInner}
            accessible
            accessibilityRole="button"
            accessibilityLabel={`@${displayName}`}
          >
            <Avatar name={displayName} size={28} />
            <View style={styles.rowText}>
              <Text variant="callout" numberOfLines={1} style={styles.name}>
                {`@${displayName}`}
              </Text>
              {item.role !== undefined && (
                <Text variant="footnote" color="textMuted" numberOfLines={1}>
                  {item.role}
                </Text>
              )}
            </View>
          </View>
        </Touchable>
      );
    }
    case 'channel': {
      return (
        <Touchable
          onPress={() => onPress(item)}
          style={styles.row}
          testID={`suggestion-channel-${item.channel.id}`}
        >
          <View
            style={styles.rowInner}
            accessible
            accessibilityRole="button"
            accessibilityLabel={`#${item.channel.name}`}
          >
            <View style={[styles.iconWrap, { backgroundColor: colors.hover }]}>
              <Icon name="textChannel" size={16} color="textSecondary" />
            </View>
            <View style={styles.rowText}>
              <Text variant="callout" numberOfLines={1} style={styles.name}>
                {`#${item.channel.name}`}
              </Text>
              {item.channel.topic !== undefined && item.channel.topic !== '' && (
                <Text variant="footnote" color="textMuted" numberOfLines={1}>
                  {item.channel.topic}
                </Text>
              )}
            </View>
          </View>
        </Touchable>
      );
    }
    case 'emoji': {
      const shortcode = item.emoji.shortcodes[0] ?? item.emoji.name;
      return (
        <Touchable
          onPress={() => onPress(item)}
          style={styles.row}
          testID={`suggestion-emoji-${shortcode}`}
        >
          <View
            style={styles.rowInner}
            accessible
            accessibilityRole="button"
            accessibilityLabel={`:${shortcode}:`}
          >
            <NativeText style={styles.emojiChar}>{item.emoji.char}</NativeText>
            <View style={styles.rowText}>
              <Text variant="callout" numberOfLines={1} style={styles.name}>
                {`:${shortcode}:`}
              </Text>
              <Text variant="footnote" color="textMuted" numberOfLines={1}>
                {item.emoji.name}
              </Text>
            </View>
          </View>
        </Touchable>
      );
    }
  }
}

const styles = StyleSheet.create({
  panel: {
    position: 'absolute',
    left: space.sm,
    right: space.sm,
    bottom: '100%',
    marginBottom: 2,
    borderRadius: 20,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
    shadowOpacity: 0.25,
    shadowRadius: 16,
    shadowOffset: { width: 0, height: 8 },
    elevation: 10,
  },
  row: {
    height: SUGGESTION_ROW_HEIGHT,
  },
  rowInner: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingHorizontal: 14,
  },
  rowText: {
    flex: 1,
    minWidth: 0,
  },
  name: {
    fontWeight: '600',
  },
  iconWrap: {
    width: 28,
    height: 28,
    borderRadius: 14,
    alignItems: 'center',
    justifyContent: 'center',
  },
  emojiChar: {
    fontSize: 22,
    width: 28,
    textAlign: 'center',
  },
});
