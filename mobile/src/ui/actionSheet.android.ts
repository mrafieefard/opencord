import { useSyncExternalStore } from 'react';

import type { ActionSheetOptions } from './actionSheet.types';

export type { ActionSheetAction, ActionSheetOptions } from './actionSheet.types';

let open: ActionSheetOptions | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function showActionSheet(options: ActionSheetOptions): void {
  open = options;
  emit();
}

export function closeActionSheet(actionId?: string): void {
  const was = open;
  open = null;
  emit();
  if (actionId) was?.onAction(actionId);
  else was?.onCancel?.();
}

export function useOpenActionSheet(): ActionSheetOptions | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => open,
  );
}
