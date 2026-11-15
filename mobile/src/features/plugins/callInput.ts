// The `call` input for an opened activity's view (plugins M4 plan, Task 15;
// §15.3): who is in the call and who speaks, as decimal ids sorted as
// numbers. Sent once the view is open, then when either list changes, at
// most once every CALL_INPUT_EVERY_MS with the newest lists.
import { CALL_INPUT_EVERY_MS } from './limits';

export interface CallLists {
  members: readonly string[];
  speaking: readonly string[];
}

function sorted(ids: readonly string[]): string[] {
  return [...new Set(ids)].sort((a, b) => {
    const [x, y] = [BigInt(a), BigInt(b)];
    return x < y ? -1 : x > y ? 1 : 0;
  });
}

export function callInputJson(lists: CallLists): string {
  return JSON.stringify({ kind: 'call', members: sorted(lists.members), speaking: sorted(lists.speaking) });
}

export interface CallInputOptions {
  read: () => CallLists;
  /** Calls `changed` when the lists may have changed; returns the stop. */
  subscribe: (changed: () => void) => () => void;
  send: (json: string) => void;
}

/** Starts sending; returns the stop. */
export function startCallInput({ read, subscribe, send }: CallInputOptions): () => void {
  let last = callInputJson(read());
  send(last);
  let lastAt = Date.now();
  let timer: ReturnType<typeof setTimeout> | null = null;
  const flush = () => {
    timer = null;
    const next = callInputJson(read());
    if (next === last) return;
    last = next;
    lastAt = Date.now();
    send(next);
  };
  const unsubscribe = subscribe(() => {
    if (timer !== null) return;
    timer = setTimeout(flush, Math.max(0, lastAt + CALL_INPUT_EVERY_MS - Date.now()));
  });
  return () => {
    unsubscribe();
    if (timer !== null) clearTimeout(timer);
  };
}
