// The soundboard (Phase 2 plan §11.7, as the desktop's picker): a sheet
// over the call with a search, then Favorites, This server, each other
// server and Default, three sounds a row. A tap plays a sound for everyone
// in the channel; a long press previews it here or makes it a favourite.
// Sounds that can't be played here are dimmed, and a tap says why. A thin
// bar under the title empties as the cooldown runs out.
import { router } from 'expo-router';
import { useEffect, useState } from 'react';
import { type LayoutChangeEvent, ScrollView, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import Animated, { Easing, cancelAnimation, useAnimatedStyle, useSharedValue, withTiming } from 'react-native-reanimated';

import { report } from '@/core';
import { toggleSoundFavorite } from '@/state/actions/prefs';
import { playSound, previewSound } from '@/state/actions/soundboard';
import { useLastSound, useSoundboardPicker } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import {
  EmptyState,
  Icon,
  Menu,
  SearchField,
  SheetHeader,
  Text,
  radius,
  sheetLook,
  space,
  useSheetBottomInset,
  useTheme,
  type MenuAction,
} from '@/ui';

import type { PickerSound } from './picker';

const COLUMNS = 3;
const GAP = space.sm;
const TILE_HEIGHT = 76;

export interface SoundboardSheetProps {
  onClose?: () => void;
}

export function SoundboardSheet({ onClose }: SoundboardSheetProps) {
  const { colors } = useTheme();
  const bottomInset = useSheetBottomInset();
  const [query, setQuery] = useState('');
  const window = useWindowDimensions();
  // The window's width until the grid is laid out: a sheet is as wide on a phone.
  const [width, setWidth] = useState(() => window.width - space.lg * 2);
  const picker = useSoundboardPicker(query);
  const favorites = useAppStore((state) => state.prefs.soundFavorites);
  const close = onClose ?? (() => router.back());

  const onLayout = (event: LayoutChangeEvent) => {
    const next = event.nativeEvent.layout.width;
    if (next > 0 && next !== width) setWidth(next);
  };
  const tileWidth = width > 0 ? Math.floor((width - GAP * (COLUMNS - 1)) / COLUMNS) : 0;

  // Kept as its own view: flattened, its children land in the sheet's own
  // container, and react-native-screens then places the ScrollView at the
  // top of the sheet, over the title and search (iOS 26; as MusicPlayer).
  return (
    <View collapsable={false} style={[styles.sheet, { backgroundColor: sheetLook.background(colors) }]}>
      <SheetHeader title="Soundboard" onClose={close} />
      {picker ? (
        <>
          <CooldownBar serverKey={picker.serverKey} cooldownMs={picker.cooldownMs} />
          <View style={styles.search}>
            <SearchField value={query} onChangeText={setQuery} placeholder="Search sounds" onSheet />
          </View>
          {picker.blocked !== null && (
            <Text variant="footnote" color="textSecondary" style={styles.blocked}>
              {picker.blocked}
            </Text>
          )}
          <ScrollView
            style={styles.fill}
            contentContainerStyle={[styles.content, { paddingBottom: bottomInset + space.lg }]}
            keyboardDismissMode="on-drag"
            keyboardShouldPersistTaps="handled"
          >
            <View onLayout={onLayout}>
              {picker.empty ? (
                <EmptyState title={query.trim() === '' ? 'No sounds yet' : 'No sounds match'} />
              ) : (
                tileWidth > 0 &&
                picker.sections.map((section) => (
                  <View key={section.title} style={styles.section}>
                    <Text variant="footnote" color="textSecondary" accessibilityRole="header" style={styles.sectionTitle}>
                      {section.title}
                    </Text>
                    <View style={styles.grid}>
                      {section.sounds.map((sound) => (
                        <SoundTile
                          key={`${section.title}-${sound.favorite}`}
                          sound={sound}
                          width={tileWidth}
                          favorite={favorites.includes(sound.favorite)}
                        />
                      ))}
                    </View>
                  </View>
                ))
              )}
            </View>
          </ScrollView>
        </>
      ) : (
        <EmptyState title="Join a voice channel to play sounds" />
      )}
    </View>
  );
}

interface SoundTileProps {
  sound: PickerSound;
  width: number;
  favorite: boolean;
}

function SoundTile({ sound, width, favorite }: SoundTileProps) {
  const { colors } = useTheme();
  const dimmed = sound.reason !== null;
  const actions: MenuAction[] = [
    { id: 'preview', title: 'Preview', icon: 'play' },
    { id: 'favorite', title: favorite ? 'Remove from Favorites' : 'Add to Favorites', icon: favorite ? 'starFill' : 'star' },
  ];
  const onAction = (id: string) => {
    if (id === 'preview') report(previewSound(sound), 'preview a sound');
    else toggleSoundFavorite(sound.favorite);
  };
  return (
    <Menu actions={actions} onAction={onAction} trigger="longPress" fit="content" testID={`sound-menu-${sound.favorite}`}>
      <Touchable
        onPress={() => void playSound(sound)}
        accessible
        accessibilityRole="button"
        accessibilityLabel={dimmed ? `${sound.name}, ${sound.reason}` : `Play ${sound.name}`}
        accessibilityState={{ disabled: dimmed }}
        testID={`sound-${sound.favorite}`}
      >
        <View style={[styles.tile, { width, backgroundColor: sheetLook.well(colors) }]}>
          <View style={[styles.face, dimmed && styles.dimmed]}>
            {sound.emoji ? (
              <Text variant="title2" style={styles.emoji}>
                {sound.emoji}
              </Text>
            ) : (
              <Icon name="music" size={22} color="textSecondary" />
            )}
            <Text variant="footnote" numberOfLines={1} style={styles.name} color={dimmed ? 'textMuted' : 'text'}>
              {sound.name}
            </Text>
          </View>
          {favorite && (
            <View style={styles.star}>
              <Icon name="starFill" size={11} color="textSecondary" />
            </View>
          )}
        </View>
      </Touchable>
    </Menu>
  );
}

/** A thin bar that empties as the cooldown runs out. */
function CooldownBar({ serverKey, cooldownMs }: { serverKey: string; cooldownMs: number }) {
  const { colors } = useTheme();
  const last = useLastSound(serverKey);
  const left = useSharedValue(0);
  useEffect(() => {
    if (last === undefined || cooldownMs <= 0) return;
    const remaining = cooldownMs - (Date.now() - last);
    if (remaining <= 0) return;
    left.set(remaining / cooldownMs);
    left.set(withTiming(0, { duration: remaining, easing: Easing.linear }));
    return () => cancelAnimation(left);
  }, [last, cooldownMs, left]);
  const style = useAnimatedStyle(() => ({ transform: [{ scaleX: left.get() }] }));
  return (
    <View style={styles.cooldown}>
      <Animated.View style={[styles.cooldownFill, { backgroundColor: colors.text }, style]} />
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: { flex: 1 },
  fill: { flex: 1 },
  search: { paddingHorizontal: space.lg, paddingTop: space.sm },
  blocked: { paddingHorizontal: space.lg, paddingTop: space.sm },
  content: { paddingHorizontal: space.lg },
  section: { marginTop: space.lg },
  sectionTitle: { marginBottom: space.sm, marginLeft: space.xs },
  grid: { flexDirection: 'row', flexWrap: 'wrap', gap: GAP },
  tile: {
    height: TILE_HEIGHT,
    borderRadius: radius.tile,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: space.sm,
  },
  face: { alignItems: 'center', gap: space.xs, maxWidth: '100%' },
  dimmed: { opacity: 0.4 },
  emoji: { fontSize: 24, lineHeight: 30 },
  name: { maxWidth: '100%' },
  star: { position: 'absolute', top: space.xs + 2, right: space.xs + 2 },
  cooldown: { height: 2, marginHorizontal: space.lg, overflow: 'hidden' },
  // Scaled from its left edge: a full-width bar, shrunk.
  cooldownFill: { height: 2, width: '100%', transformOrigin: 'left' },
});
