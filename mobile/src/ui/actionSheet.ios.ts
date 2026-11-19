import { ActionSheetIOS } from 'react-native';

import type { ActionSheetOptions } from './actionSheet.types';

export type { ActionSheetAction, ActionSheetOptions } from './actionSheet.types';

export function showActionSheet({ title, actions, onAction, onCancel }: ActionSheetOptions): void {
  const options = ['Cancel', ...actions.map((a) => a.title)];
  const destructiveIndex = actions.findIndex((a) => a.destructive);
  ActionSheetIOS.showActionSheetWithOptions(
    {
      title,
      options,
      cancelButtonIndex: 0,
      destructiveButtonIndex: destructiveIndex >= 0 ? destructiveIndex + 1 : undefined,
    },
    (buttonIndex) => {
      if (buttonIndex === 0) {
        onCancel?.();
      } else {
        const action = actions[buttonIndex - 1];
        if (action) onAction(action.id);
        else onCancel?.();
      }
    },
  );
}
