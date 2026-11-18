// The emoji picker (design §4.5, §4.6, P2 Task 3): a sheet at half
// height with search, category tabs as icons, 8 columns grid, and
// recent emoji first (kept in prefs, at most 24). A tap inserts at the
// caret and keeps the sheet open; the keyboard comes back when it closes.
import { type ComponentRef, useMemo, useRef, useState } from 'react';
import {
  Modal,
  ScrollView,
  StyleSheet,
  Text as NativeText,
  useWindowDimensions,
  View,
} from 'react-native';
import { GestureHandlerRootView, Touchable } from 'react-native-gesture-handler';

import { recordEmojiUsed } from '@/state/actions/prefs';
import { usePrefs } from '@/state/selectors';
import {
  Icon,
  radius,
  SearchField,
  SheetHeader,
  sheetLook,
  space,
  Text,
  useTheme,
  type IconName,
} from '@/ui';

import {
  CATEGORY_LABELS,
  DEFAULT_REACTIONS,
  EMOJI_CATEGORIES,
  emojiData,
  searchEmoji,
  type Emoji,
  type EmojiCategory,
} from './emoji';

export interface EmojiPickerProps {
  onPick: (emoji: string) => void;
  onClose?: () => void;
  visible?: boolean;
}

const COLUMNS = 8;

const CATEGORY_ICONS: Record<EmojiCategory, IconName> = {
  smileys: 'categorySmileys',
  people: 'categoryPeople',
  nature: 'categoryNature',
  food: 'categoryFood',
  activities: 'categoryActivities',
  travel: 'categoryTravel',
  objects: 'categoryObjects',
  symbols: 'categorySymbols',
  flags: 'categoryFlags',
};

export function EmojiPicker({ onPick, onClose, visible }: EmojiPickerProps) {
  const { colors } = useTheme();
  const window = useWindowDimensions();
  const [query, setQuery] = useState('');
  const [activeCategory, setActiveCategory] = useState<string>('recent');
  const scrollRef = useRef<ComponentRef<typeof ScrollView>>(null);
  const prefs = usePrefs();

  const recentList = useMemo(() => {
    const list = prefs.recentEmoji ?? [];
    const combined = [...list, ...DEFAULT_REACTIONS.filter((emoji) => !list.includes(emoji))];
    return combined.slice(0, 24);
  }, [prefs.recentEmoji]);

  const searchResults = useMemo(() => {
    if (query.trim() === '') return [];
    return searchEmoji(query, 160);
  }, [query]);

  const byCategory = useMemo(() => {
    const map = new Map<EmojiCategory, Emoji[]>();
    for (const cat of EMOJI_CATEGORIES) {
      map.set(cat, []);
    }
    for (const e of emojiData) {
      map.get(e.category)?.push(e);
    }
    return map;
  }, []);

  const handlePick = (char: string) => {
    onPick(char);
    recordEmojiUsed(char);
  };

  const handleCategoryPress = (category: string) => {
    setActiveCategory(category);
    // When switching category tabs, reset query
    if (query !== '') setQuery('');
    scrollRef.current?.scrollTo({ y: 0, animated: true });
  };

  const sheetHeight = Math.round(window.height * 0.5);

  const sheetContent = (
    <View
      testID="emoji-picker"
      style={[
        styles.sheet,
        {
          backgroundColor: sheetLook.background(colors),
          height: sheetHeight,
        },
      ]}
    >
      <SheetHeader title="Emoji" onClose={onClose ?? (() => {})} />
      <View style={styles.searchRow} testID="emoji-search">
        <SearchField
          onSheet
          value={query}
          onChangeText={setQuery}
          placeholder="Search emoji"
        />
      </View>

      {query === '' && (
        <View style={[styles.tabsRow, { borderBottomColor: colors.border }]}>
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            contentContainerStyle={styles.tabsContent}
          >
            <Touchable
              onPress={() => handleCategoryPress('recent')}
              style={[
                styles.tabButton,
                activeCategory === 'recent' && { backgroundColor: colors.selected },
              ]}
              accessibilityLabel="Frequently used"
              accessibilityRole="tab"
            >
              <Icon
                name="clock"
                size={20}
                color={activeCategory === 'recent' ? 'text' : 'textSecondary'}
              />
            </Touchable>
            {EMOJI_CATEGORIES.map((cat) => (
              <Touchable
                key={cat}
                onPress={() => handleCategoryPress(cat)}
                style={[
                  styles.tabButton,
                  activeCategory === cat && { backgroundColor: colors.selected },
                ]}
                accessibilityLabel={CATEGORY_LABELS[cat]}
                accessibilityRole="tab"
              >
                <Icon
                  name={CATEGORY_ICONS[cat]}
                  size={20}
                  color={activeCategory === cat ? 'text' : 'textSecondary'}
                />
              </Touchable>
            ))}
          </ScrollView>
        </View>
      )}

      <ScrollView
        ref={scrollRef}
        nestedScrollEnabled
        style={styles.gridScroll}
        contentContainerStyle={styles.gridContent}
      >
        {query !== '' ? (
          <View style={styles.grid}>
            {searchResults.map((emoji, index) => (
              <Touchable
                key={`${emoji.char}-${index}`}
                onPress={() => handlePick(emoji.char)}
                style={styles.cell}
                accessibilityLabel={emoji.name}
                testID={`emoji-${emoji.char}`}
              >
                <NativeText style={styles.emojiText}>{emoji.char}</NativeText>
              </Touchable>
            ))}
          </View>
        ) : (
          <>
            {activeCategory === 'recent' && (
              <View style={styles.section}>
                <Text variant="subhead" color="textSecondary" style={styles.sectionHeader}>
                  Frequently used
                </Text>
                <View style={styles.grid}>
                  {recentList.map((char, index) => (
                    <Touchable
                      key={`recent-${char}-${index}`}
                      onPress={() => handlePick(char)}
                      style={styles.cell}
                      accessibilityLabel={`Recent ${char}`}
                      testID={`emoji-recent-${char}`}
                    >
                      <NativeText style={styles.emojiText}>{char}</NativeText>
                    </Touchable>
                  ))}
                </View>
              </View>
            )}

            {EMOJI_CATEGORIES.filter(
              (cat) => activeCategory === cat || (activeCategory === 'recent' && cat === 'smileys'),
            ).map((cat) => {
              const items = byCategory.get(cat) ?? [];
              if (items.length === 0) return null;
              return (
                <View key={cat} style={styles.section}>
                  <Text variant="subhead" color="textSecondary" style={styles.sectionHeader}>
                    {CATEGORY_LABELS[cat]}
                  </Text>
                  <View style={styles.grid}>
                    {items.map((emoji) => (
                      <Touchable
                        key={`${cat}-${emoji.char}`}
                        onPress={() => handlePick(emoji.char)}
                        style={styles.cell}
                        accessibilityLabel={emoji.name}
                        testID={`emoji-${emoji.char}`}
                      >
                        <NativeText style={styles.emojiText}>{emoji.char}</NativeText>
                      </Touchable>
                    ))}
                  </View>
                </View>
              );
            })}
          </>
        )}
      </ScrollView>
    </View>
  );

  if (visible === false) {
    return null;
  }

  if (visible === undefined) {
    return sheetContent;
  }

  return (
    <Modal
      transparent
      visible={visible}
      statusBarTranslucent
      navigationBarTranslucent
      animationType="slide"
      onRequestClose={onClose}
    >
      <GestureHandlerRootView style={styles.modalOverlay}>
        <Touchable
          onPress={onClose}
          style={StyleSheet.absoluteFill}
          accessibilityLabel="Close"
        />
        {sheetContent}
      </GestureHandlerRootView>
    </Modal>
  );
}

const styles = StyleSheet.create({
  modalOverlay: {
    flex: 1,
    justifyContent: 'flex-end',
    backgroundColor: 'rgba(0, 0, 0, 0.4)',
  },
  sheet: {
    borderTopLeftRadius: radius.sheet,
    borderTopRightRadius: radius.sheet,
    paddingHorizontal: space.md,
    paddingBottom: space.md,
    overflow: 'hidden',
  },
  searchRow: {
    paddingVertical: space.xs,
  },
  tabsRow: {
    borderBottomWidth: StyleSheet.hairlineWidth,
    paddingVertical: space.xs,
  },
  tabsContent: {
    gap: space.xs,
  },
  tabButton: {
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    borderRadius: radius.pill,
  },
  gridScroll: {
    flex: 1,
  },
  gridContent: {
    paddingVertical: space.sm,
  },
  section: {
    marginBottom: space.md,
  },
  sectionHeader: {
    paddingVertical: space.xs,
    paddingHorizontal: space.xs,
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
  },
  cell: {
    width: `${100 / COLUMNS}%`,
    aspectRatio: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  emojiText: {
    fontSize: 26,
  },
});
