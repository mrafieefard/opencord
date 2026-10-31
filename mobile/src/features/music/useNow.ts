// The clock the player's position moves by (spec §8.7): every 500 ms while
// something plays, one timer however many players listen; 0 otherwise.
import { useSyncExternalStore } from 'react';

const TICK_MS = 500;
let now = 0;
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (!timer) {
    now = Date.now();
    timer = setInterval(() => {
      now = Date.now();
      listeners.forEach((each) => each());
    }, TICK_MS);
    // Never what keeps Jest (or anything else) running.
    timer.unref?.();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer) {
      clearInterval(timer);
      timer = null;
    }
  };
}

const idle = () => () => {};

export function useNow(running: boolean): number {
  return useSyncExternalStore(
    running ? subscribe : idle,
    () => (running ? now : 0),
    () => 0,
  );
}
