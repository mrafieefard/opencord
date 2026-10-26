// A native menu (UIMenu on iOS, PopupMenu on Android) on a press or a long
// press (design §10).
import { MenuView } from '@react-native-menu/menu';
import type { ReactNode } from 'react';
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
  children: ReactNode;
}

export function Menu({ actions, onAction, trigger = 'press', children }: MenuProps) {
  const { colors, scheme } = useTheme();
  return (
    <MenuView
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
