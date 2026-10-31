// A question before something that can't be undone. Android's AlertDialog
// ignores the app's theme (a grey box, teal capitals), so the question is
// held here and DialogHost draws it in the app's style.
import { useSyncExternalStore } from 'react';

import type { ConfirmOptions } from './confirm.types';

export type { ConfirmOptions } from './confirm.types';

let open: ConfirmOptions | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function confirm(options: ConfirmOptions): void {
  open = options;
  emit();
}

export function closeConfirm(): void {
  open = null;
  emit();
}

export function useOpenConfirm(): ConfirmOptions | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => open,
  );
}
