// One open plugin view on the phone (plugins M4 plan, Task 7): its area,
// opened with the renderer's first context input, batches to the area and
// inputs to the core. The phone's pluginViewProvider.
import type * as PluginArea from '@opencord/plugin-area';

import type { CoreApi } from '@/core/api';
import { errorKind } from '@/core/errors';

import type { Hold, ViewPump } from './viewPump';

export type AreaModule = Pick<
  typeof PluginArea,
  | 'createArea'
  | 'destroyArea'
  | 'destroyAllAreas'
  | 'setSetup'
  | 'pushBatch'
  | 'hostReply'
  | 'back'
  | 'addInputListener'
  | 'addStatusListener'
  | 'addHostCallListener'
>;

export type ViewPlace =
  | { kind: 'channel'; serverKey: string; channelId: string }
  | { kind: 'activity'; serverKey: string; sessionId: string; channelId: string; activityKey: string };

/** What React Native draws around the area ("The plugin area channel", status). */
export interface AreaStatus {
  state: 'loading' | 'running' | 'error' | 'too_old' | 'closed';
  reason: string;
  detail: string;
  overlays: number;
}

export interface ViewSessionOptions {
  core: CoreApi;
  pump: ViewPump;
  area: AreaModule;
  place: ViewPlace;
  setupJson: string;
}

const LOADING: AreaStatus = { state: 'loading', reason: '', detail: '', overlays: 0 };

// Area ids are unique across JavaScript starts: an old session's cleanup
// must never reach a new area.
const startedAt = Date.now();
let sessions = 0;

export class ViewSession {
  readonly areaId = `area-${startedAt}-${++sessions}`;
  status: AreaStatus = LOADING;
  viewId: string | null = null;
  private readonly options: ViewSessionOptions;
  private readonly subscriptions: { remove(): void }[] = [];
  private readonly statusListeners = new Set<(status: AreaStatus) => void>();
  private readonly waiting: string[] = [];
  private stopListening: () => void = () => {};
  private opening = false;
  private closed = false;

  constructor(options: ViewSessionOptions) {
    this.options = options;
  }

  /** Creates the area and listens to it; resolves when Dart is ready. */
  async start(): Promise<void> {
    const { area } = this.options;
    this.subscriptions.push(
      area.addInputListener((event) => {
        if (event.areaId === this.areaId) this.fromRenderer(event.json);
      }),
      area.addStatusListener((event) => {
        if (event.areaId === this.areaId) this.setStatus(JSON.parse(event.json) as AreaStatus);
      }),
    );
    try {
      await area.createArea(this.areaId, this.options.setupJson);
    } catch (error: unknown) {
      console.warn(`plugin area failed to start: ${errorKind(error)}`);
      this.fail();
    }
  }

  /** An input React Native originates (`call`, lifecycle). */
  send(input: object): void {
    this.toCore(JSON.stringify(input));
  }

  subscribe(listener: (status: AreaStatus) => void): () => void {
    this.statusListeners.add(listener);
    return () => this.statusListeners.delete(listener);
  }

  /** Closes the view and the area; safe twice and while opening. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    for (const subscription of this.subscriptions.splice(0)) subscription.remove();
    this.stopListening();
    if (this.viewId !== null) this.options.core.viewClose(this.viewId);
    this.options.area.destroyArea(this.areaId);
    this.statusListeners.clear();
  }

  private fromRenderer(json: string) {
    if (this.viewId !== null || this.opening) {
      this.toCore(json);
      return;
    }
    const input = JSON.parse(json) as { kind?: string };
    if (input.kind !== 'context') {
      this.waiting.push(json);
      return;
    }
    const { kind: _kind, ...context } = input;
    void this.open(JSON.stringify(context));
  }

  private toCore(json: string) {
    if (this.closed) return;
    if (this.viewId === null) {
      this.waiting.push(json);
      return;
    }
    this.options.core.viewInput(this.viewId, json);
  }

  private async open(contextJson: string) {
    const { core, pump, area, place } = this.options;
    this.opening = true;
    const hold: Hold = pump.hold();
    let viewId: string;
    try {
      viewId =
        place.kind === 'channel'
          ? await core.viewOpen(place.serverKey, place.channelId, contextJson)
          : await core.viewOpenActivity(place.serverKey, place.sessionId, place.channelId, place.activityKey, contextJson);
    } catch (error: unknown) {
      hold.release(null);
      console.warn(`plugin view failed to open: ${errorKind(error)}`);
      this.fail();
      return;
    }
    if (this.closed) {
      hold.release(null);
      core.viewClose(viewId);
      return;
    }
    this.viewId = viewId;
    this.stopListening = pump.listen(viewId, (json) => area.pushBatch(this.areaId, json));
    hold.release(viewId);
    for (const json of this.waiting.splice(0)) core.viewInput(viewId, json);
  }

  private fail() {
    this.setStatus({ state: 'error', reason: '', detail: '', overlays: 0 });
    this.close();
  }

  private setStatus(status: AreaStatus) {
    this.status = status;
    for (const listener of [...this.statusListeners]) listener(status);
  }
}
