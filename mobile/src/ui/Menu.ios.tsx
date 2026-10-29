// UIMenu on a press or a long press (design §10). It opens from its own
// press and long-press interactions, so `open()` does nothing.
import { MenuView } from '@react-native-menu/menu';

import { icons } from './icons';
import type { MenuProps } from './Menu.types';
import { useTheme } from './theme/useTheme';

export { useMenuOpener, type MenuAction, type MenuHandle, type MenuProps } from './Menu.types';

export function Menu({ actions, onAction, trigger = 'press', testID, children }: MenuProps) {
  const { colors, scheme } = useTheme();
  return (
    <MenuView
      testID={testID}
      shouldOpenOnLongPress={trigger === 'longPress'}
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
