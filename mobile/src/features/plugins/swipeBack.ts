// Whether a plugin on screen holds the iOS swipe-back (plugins M4, §12.5):
// while it could take back (an overlay open, or `app.on_back`), the edge
// swipe would leave the screen without asking it. Each shown area holds or
// lets go; the server screen turns its stack gesture off while any holds.
import { useSyncExternalStore } from 'react';

const holders = new Set<symbol>();
const listeners = new Set<() => void>();

/** Holds the swipe-back for `owner` while `held`; returns the let-go. */
export function holdSwipeBack(owner: symbol, held: boolean): () => void {
  const was = holders.has(owner);
  if (held) holders.add(owner);
  else holders.delete(owner);
  if (was !== held) for (const listener of [...listeners]) listener();
  return () => {
    if (holders.delete(owner)) for (const listener of [...listeners]) listener();
  };
}

export function swipeBackHeld(): boolean {
  return holders.size > 0;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useSwipeBackHeld(): boolean {
  return useSyncExternalStore(subscribe, swipeBackHeld);
}
