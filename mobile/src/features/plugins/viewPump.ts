// Pulls plugin views' batches from the core (plugins M4 plan, Task 7), as
// startEventLoop pulls events: attach once, loop on nextViewOutputs, retry a
// failure with backoff, end on an empty batch (a newer pump took over). A
// batch goes to its view's listener; while a view is opening, a hold keeps
// the batches nobody listens to yet.
import type { CoreApi, ViewOutputRecord } from '@/core/api';
import { errorKind } from '@/core/errors';

import { VIEW_BATCH_MAX } from './limits';
import type { AreaModule } from './viewSession';

const FIRST_RETRY_MS = 250;
const LAST_RETRY_MS = 5000;

export interface Hold {
  /** Hands the kept batches for [viewId] to its listener, in order, and drops the rest. */
  release(viewId: string | null): void;
}

export interface ViewPump {
  /** Batches for [viewId] go to [onBatch] from now on; the result stops that. */
  listen(viewId: string, onBatch: (json: string) => void): () => void;
  /** Keeps every batch whose view has no listener until released. */
  hold(): Hold;
  stop(): void;
}

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export function startViewPump(core: CoreApi, area: Pick<AreaModule, 'destroyAllAreas'>): ViewPump {
  // Areas an earlier JavaScript start left behind; the core closes their
  // views when this pump attaches.
  area.destroyAllAreas();
  const generation = core.viewOutputsAttach();
  const listeners = new Map<string, (json: string) => void>();
  const holds = new Set<ViewOutputRecord[]>();
  let stopped = false;

  const deliver = (record: ViewOutputRecord) => {
    const listener = listeners.get(record.viewId);
    if (listener) {
      listener(record.json);
      return;
    }
    for (const kept of holds) kept.push(record);
  };

  void (async () => {
    let retryMs = FIRST_RETRY_MS;
    while (!stopped) {
      let batch: ViewOutputRecord[];
      try {
        batch = await core.nextViewOutputs(generation, VIEW_BATCH_MAX);
      } catch (error: unknown) {
        console.warn(`next view outputs failed: ${errorKind(error)}; retrying in ${retryMs} ms`);
        await wait(retryMs);
        retryMs = Math.min(retryMs * 2, LAST_RETRY_MS);
        continue;
      }
      retryMs = FIRST_RETRY_MS;
      if (stopped || batch.length === 0) return;
      for (const record of batch) deliver(record);
    }
  })();

  return {
    listen(viewId, onBatch) {
      listeners.set(viewId, onBatch);
      return () => {
        if (listeners.get(viewId) === onBatch) listeners.delete(viewId);
      };
    },
    hold() {
      const kept: ViewOutputRecord[] = [];
      holds.add(kept);
      return {
        release(viewId) {
          if (!holds.delete(kept)) return;
          if (viewId === null) return;
          const listener = listeners.get(viewId);
          if (!listener) return;
          for (const record of kept) if (record.viewId === viewId) listener(record.json);
        },
      };
    },
    stop() {
      stopped = true;
      listeners.clear();
      holds.clear();
    },
  };
}
