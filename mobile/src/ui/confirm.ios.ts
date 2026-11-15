// A question before something that can't be undone: UIAlertController,
// which already looks as iOS should.
import { Alert } from 'react-native';

import type { ConfirmOptions } from './confirm.types';

export type { ConfirmOptions } from './confirm.types';

export function confirm({ title, message, confirmTitle, destructive = false, onConfirm, onCancel }: ConfirmOptions): void {
  Alert.alert(title, message, [
    { text: 'Cancel', style: 'cancel', onPress: onCancel },
    { text: confirmTitle, style: destructive ? 'destructive' : 'default', onPress: onConfirm },
  ]);
}
