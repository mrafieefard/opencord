// A native menu (UIMenu on iOS, PopupMenu on Android) on a press or a long
// press (design §10).
//
// Inside a Gesture Handler row the row takes the touch, so Android's menu
// never sees the long press: the row's onLongPress calls `open()`. iOS
// opens the menu from its own long-press interaction, so `open()` does
// nothing there.
import { MenuView, type MenuComponentRef } from '@react-native-menu/menu';
import { useImperativeHandle, useRef, type ReactNode, type Ref } from 'react';
import { Platform } from 'react-native';

import { icons, type IconName } from './icons';
import { useTheme } from './theme/useTheme';

export interface MenuAction {
  id: string;
  title: string;
  icon?: IconName;
  destructive?: boolean;
  disabled?: boolean;
}

export interface MenuProps {
  actions: MenuAction[];
  onAction: (id: string) => void;
  trigger?: 'press' | 'longPress';
  testID?: string;
  ref?: Ref<MenuHandle>;
  children: ReactNode;
}

export interface MenuHandle {
  open(): void;
}

export function Menu({ actions, onAction, trigger = 'press', testID, ref, children }: MenuProps) {
  const { colors, scheme } = useTheme();
  const menu = useRef<MenuComponentRef>(null);
  useImperativeHandle(ref, () => ({
    open: () => {
      if (Platform.OS === 'android') menu.current?.show();
    },
  }));
  return (
    <MenuView
      ref={menu}
      testID={testID}
      shouldOpenOnLongPress={trigger === 'longPress'}
      themeVariant={scheme}
      onPressAction={({ nativeEvent }) => onAction(nativeEvent.event)}
      actions={actions.map((action) => ({
        id: action.id,
        title: action.title,
        image: Platform.OS === 'ios' && action.icon ? icons[action.icon].ios : undefined,
        imageColor: action.destructive ? colors.recording : colors.text,
        attributes: { destructive: action.destructive, disabled: action.disabled },
      }))}
    >
      {children}
    </MenuView>
  );
}
