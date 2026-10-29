// The app's own menu on Android (design §10, D78). PopupMenu can't take the
// app's colors, font or icons, so the menu is drawn here: a card in
// `elevated` beside what was pressed, over a light scrim, growing from the
// corner nearest it.
//
// It opens beside the whole view, on the view's side, so a bubble's menu
// lines up with the bubble. A child with its own touch handling (a picture
// in a bubble) opens it through `useMenuOpener`, and a Gesture Handler row
// through the ref's `open()`: the row takes the touch before this view's
// gesture sees it.
import * as Haptics from 'expo-haptics';
import { useCallback, useEffect, useImperativeHandle, useRef, useState, type ComponentRef } from 'react';
import { Modal, StyleSheet, View, useWindowDimensions } from 'react-native';
import {
  GestureDetector,
  GestureHandlerRootView,
  Touchable,
  useLongPressGesture,
  useTapGesture,
} from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withTiming } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Icon } from './Icon';
import { MenuOpener, type MenuAction, type MenuProps } from './Menu.types';
import { placeMenu, type Rect } from './menuPlacement';
import { Text } from './Text';
import { radius, space } from './theme/space';
import { useTheme } from './theme/useTheme';

export { useMenuOpener, type MenuAction, type MenuHandle, type MenuProps } from './Menu.types';

export const MENU_WIDTH = 232;
export const MENU_ROW = 48;
const MENU_PAD = 6;
/** The scrim's share of the theme's `scrim`: lighter than a sheet's. */
const SCRIM = 0.5;
const OPEN_MS = 160;

export function Menu({ actions, onAction, trigger = 'press', testID, ref, children }: MenuProps) {
  const wrapper = useRef<ComponentRef<typeof View>>(null);
  const [anchor, setAnchor] = useState<Rect | null>(null);

  const showAtView = useCallback(() => {
    wrapper.current?.measureInWindow((x, y, width, height) => setAnchor({ x, y, width, height }));
  }, []);
  const showOnLongPress = useCallback(() => {
    void Haptics.performAndroidHapticsAsync(Haptics.AndroidHaptics.Long_Press);
    showAtView();
  }, [showAtView]);

  useImperativeHandle(ref, () => ({ open: showAtView }), [showAtView]);

  const longPress = useLongPressGesture({
    enabled: trigger === 'longPress',
    runOnJS: true,
    onActivate: showOnLongPress,
  });
  const tap = useTapGesture({
    enabled: trigger === 'press',
    runOnJS: true,
    onActivate: showAtView,
  });

  const target = (
    <View ref={wrapper} collapsable={false} testID={testID}>
      {children}
    </View>
  );
  const choose = (id: string) => {
    setAnchor(null);
    onAction(id);
  };

  return (
    <MenuOpener.Provider value={showAtView}>
      {trigger === 'press' ? (
        <GestureDetector gesture={tap}>{target}</GestureDetector>
      ) : (
        <GestureDetector gesture={longPress}>{target}</GestureDetector>
      )}
      {anchor !== null && (
        <MenuCard anchor={anchor} actions={actions} onChoose={choose} onClose={() => setAnchor(null)} />
      )}
    </MenuOpener.Provider>
  );
}

interface MenuCardProps {
  anchor: Rect;
  actions: MenuAction[];
  onChoose: (id: string) => void;
  onClose: () => void;
}

function MenuCard({ anchor, actions, onChoose, onClose }: MenuCardProps) {
  const { colors } = useTheme();
  const window = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const height = actions.length * MENU_ROW + MENU_PAD * 2;
  const place = placeMenu(anchor, { width: MENU_WIDTH, height }, {
    width: window.width,
    height: window.height,
    top: insets.top,
    bottom: insets.bottom,
  });

  // The card is new each time the menu opens, so it grows in on mount.
  const progress = useSharedValue(0);
  useEffect(() => {
    progress.set(withTiming(1, { duration: OPEN_MS }));
  }, [progress]);
  const scrimStyle = useAnimatedStyle(() => ({ opacity: progress.get() * SCRIM }));
  const cardStyle = useAnimatedStyle(() => ({
    opacity: progress.get(),
    transform: [{ scale: 0.92 + 0.08 * progress.get() }],
  }));

  return (
    <Modal transparent visible statusBarTranslucent navigationBarTranslucent animationType="none" onRequestClose={onClose}>
      <GestureHandlerRootView style={styles.fill}>
        <Touchable onPress={onClose} style={styles.fill} accessibilityLabel="Close menu">
          <Animated.View style={[styles.fill, { backgroundColor: colors.scrim }, scrimStyle]} />
        </Touchable>
        <Animated.View
          accessibilityRole="menu"
          style={[
            styles.card,
            { left: place.left, top: place.top, backgroundColor: colors.elevated, transformOrigin: place.origin },
            cardStyle,
          ]}
        >
          {actions.map((action) => (
            <MenuRow key={action.id} action={action} onPress={() => onChoose(action.id)} />
          ))}
        </Animated.View>
      </GestureHandlerRootView>
    </Modal>
  );
}

function MenuRow({ action, onPress }: { action: MenuAction; onPress: () => void }) {
  const { colors } = useTheme();
  const color = action.disabled ? 'textMuted' : action.destructive ? 'recording' : 'text';
  return (
    <Touchable
      onPress={onPress}
      disabled={action.disabled}
      underlayColor={colors.selected}
      activeUnderlayOpacity={1}
      style={styles.row}
    >
      <View
        accessible
        accessibilityRole="menuitem"
        accessibilityLabel={action.title}
        accessibilityState={{ disabled: action.disabled }}
        style={styles.rowContent}
      >
        {action.icon && (
          <Icon name={action.icon} size={20} color={action.destructive || action.disabled ? color : 'textSecondary'} />
        )}
        <Text variant="body" color={color} numberOfLines={1} style={styles.title}>
          {action.title}
        </Text>
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  fill: StyleSheet.absoluteFill,
  card: {
    position: 'absolute',
    width: MENU_WIDTH,
    paddingVertical: MENU_PAD,
    borderRadius: radius.icon,
    overflow: 'hidden',
  },
  row: { height: MENU_ROW, justifyContent: 'center' },
  rowContent: { flexDirection: 'row', alignItems: 'center', gap: space.md, paddingHorizontal: space.lg },
  title: { flex: 1 },
});
