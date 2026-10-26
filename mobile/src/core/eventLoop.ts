// Pulls the core's events in batches (design §4): attach once, then loop on
// nextEvents. An empty batch means a newer loop took the stream over (after
// a reload), so this one ends. A failed call waits and tries again.
import type { CoreApi, MobileEvent } from './api';
import { errorKind } from './errors';

export const BATCH_MAX = 256;
const FIRST_RETRY_MS = 250;
const LAST_RETRY_MS = 5000;

export interface EventLoop {
  stop(): void;
}

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export function startEventLoop(core: CoreApi, onBatch: (events: MobileEvent[]) => void): EventLoop {
  const generation = core.eventsAttach();
  let stopped = false;
  void (async () => {
    let retryMs = FIRST_RETRY_MS;
    while (!stopped) {
      let batch: MobileEvent[];
      try {
        batch = await core.nextEvents(generation, BATCH_MAX);
      } catch (error: unknown) {
        console.warn(`next events failed: ${errorKind(error)}; retrying in ${retryMs} ms`);
        await wait(retryMs);
        retryMs = Math.min(retryMs * 2, LAST_RETRY_MS);
        continue;
      }
      retryMs = FIRST_RETRY_MS;
      if (stopped || batch.length === 0) return;
      onBatch(batch);
    }
  })();
  return {
    stop() {
      stopped = true;
    },
  };
}
