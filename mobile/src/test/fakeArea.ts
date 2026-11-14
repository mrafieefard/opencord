// A hand-written fake of @opencord/plugin-area: records what was asked of it
// and lets a test fire Dart's ready, inputs and statuses.
import type { AreaEvent, HostCallEvent, Subscription } from '@opencord/plugin-area';

import type { AreaModule } from '@/features/plugins/viewSession';

type Listener<T> = (event: T) => void;

export class FakeArea implements AreaModule {
  readonly calls: { method: string; args: unknown[] }[] = [];
  readonly pushed = new Map<string, string[]>();
  private inputs = new Set<Listener<AreaEvent>>();
  private statuses = new Set<Listener<AreaEvent>>();
  private hostCalls = new Set<Listener<HostCallEvent>>();
  private ready = new Map<string, { resolve: () => void; reject: (error: Error) => void }>();
  /** When true, createArea resolves at once (Dart ready). */
  autoReady = true;

  private record(method: string, ...args: unknown[]) {
    this.calls.push({ method, args });
  }

  methods(): string[] {
    return this.calls.map((call) => call.method);
  }

  createArea(areaId: string, setupJson: string): Promise<void> {
    this.record('createArea', areaId, setupJson);
    if (this.autoReady) return Promise.resolve();
    return new Promise((resolve, reject) => this.ready.set(areaId, { resolve, reject }));
  }

  fireReady(areaId: string) {
    this.ready.get(areaId)?.resolve();
  }

  failCreate(areaId: string, message: string) {
    this.ready.get(areaId)?.reject(new Error(message));
  }

  destroyArea(areaId: string) {
    this.record('destroyArea', areaId);
  }

  destroyAllAreas() {
    this.record('destroyAllAreas');
  }

  setSetup(areaId: string, setupJson: string) {
    this.record('setSetup', areaId, setupJson);
  }

  pushBatch(areaId: string, json: string) {
    this.record('pushBatch', areaId, json);
    this.pushed.set(areaId, [...(this.pushed.get(areaId) ?? []), json]);
  }

  hostReply(areaId: string, id: number, json: string) {
    this.record('hostReply', areaId, id, json);
  }

  back(areaId: string): Promise<boolean> {
    this.record('back', areaId);
    return Promise.resolve(false);
  }

  addInputListener(listener: Listener<AreaEvent>): Subscription {
    this.inputs.add(listener);
    return { remove: () => this.inputs.delete(listener) };
  }

  addStatusListener(listener: Listener<AreaEvent>): Subscription {
    this.statuses.add(listener);
    return { remove: () => this.statuses.delete(listener) };
  }

  addHostCallListener(listener: Listener<HostCallEvent>): Subscription {
    this.hostCalls.add(listener);
    return { remove: () => this.hostCalls.delete(listener) };
  }

  /** Dart sends an input, as `BlocksController.inputs` gave it. */
  input(areaId: string, input: object) {
    for (const listener of [...this.inputs]) listener({ areaId, json: JSON.stringify(input) });
  }

  status(areaId: string, status: object) {
    for (const listener of [...this.statuses]) listener({ areaId, json: JSON.stringify(status) });
  }

  hostCall(event: HostCallEvent) {
    for (const listener of [...this.hostCalls]) listener(event);
  }

  listenerCount(): number {
    return this.inputs.size + this.statuses.size + this.hostCalls.size;
  }
}
