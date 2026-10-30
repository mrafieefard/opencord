// The system's own menus (design §10), opened from their own interactions,
// so `open()` does nothing.
// - On a press: UIMenu, from @react-native-menu's button.
// - On a long press: SwiftUI's context menu, under the content. The menu
//   library's button sits above its children under Fabric and takes their
//   taps, so a row inside it could never be opened.
import { Button, ContextMenu, Host, RNHostView } from '@expo/ui/swift-ui';
import { disabled } from '@expo/ui/swift-ui/modifiers';
import { MenuView } from '@react-native-menu/menu';
import { useState } from 'react';
import { type LayoutChangeEvent, View } from 'react-native';

import { icons } from './icons';
import type { MenuProps } from './Menu.types';
import { useTheme } from './theme/useTheme';

export { useMenuOpener, type MenuAction, type MenuHandle, type MenuProps } from './Menu.types';

export function Menu({ actions, onAction, trigger = 'press', fit = 'fill', background, testID, children }: MenuProps) {
  const { colors, scheme } = useTheme();
  // Content that fills its row needs a width from its first layout, or its
  // text grows past the screen's edge: it shows plainly until its width is
  // measured, then moves into the menu. Content that keeps its own width (a
  // bubble with its maxWidth) sizes the menu instead, and is never measured:
  // a recycled cell would keep another message's width.
  const [width, setWidth] = useState<number>();
  const measure = (event: LayoutChangeEvent) => {
    const measured = event.nativeEvent.layout.width;
    if (measured > 0) setWidth(measured);
  };
  if (trigger === 'longPress') {
    if (actions.length === 0) return <>{children}</>;
    const hug = fit === 'content';
    if (!hug && width === undefined) {
      return (
        <View testID={testID} onLayout={measure}>
          {children}
        </View>
      );
    }
    return (
      <View testID={testID} onLayout={hug ? undefined : measure} style={background && { backgroundColor: colors[background] }}>
        <Host matchContents={hug || { vertical: true }} colorScheme={scheme} ignoreSafeArea="all">
          <ContextMenu>
            <ContextMenu.Items>
              {actions.map((action) => (
                <Button
                  key={action.id}
                  label={action.title}
                  systemImage={action.icon ? icons[action.icon].ios : undefined}
                  role={action.destructive ? 'destructive' : 'default'}
                  modifiers={action.disabled ? [disabled()] : undefined}
                  onPress={() => onAction(action.id)}
                />
              ))}
            </ContextMenu.Items>
            <ContextMenu.Trigger>
              <RNHostView matchContents={hug || { vertical: true }}>
                <View style={hug ? undefined : { width }}>{children}</View>
              </RNHostView>
            </ContextMenu.Trigger>
          </ContextMenu>
        </Host>
      </View>
    );
  }
  return (
    <MenuView
      testID={testID}
      themeVariant={scheme}
      onPressAction={({ nativeEvent }) => onAction(nativeEvent.event)}
      actions={actions.map((action) => ({
        id: action.id,
        title: action.title,
        image: action.icon ? icons[action.icon].ios : undefined,
        imageColor: action.destructive ? colors.recording : colors.text,
        attributes: { destructive: action.destructive, disabled: action.disabled },
      }))}
    >
      {children}
    </MenuView>
  );
}
