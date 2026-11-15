// Plugin areas (plugins M4 plan, Task 4): one Flutter engine per area, from
// `createArea` to `destroyArea`, whatever views show it. The JSON strings
// pass through untouched ("The plugin area channel"). Without the native
// module (jest, a build without Flutter) `available` is false and every
// call does nothing.
import { type NativeModule, requireOptionalNativeModule } from 'expo';

import type { AreaEvent, HostCallEvent, Subscription } from './PluginArea.types';

type Events = {
  input(event: AreaEvent): void;
  status(event: AreaEvent): void;
  hostCall(event: HostCallEvent): void;
};

declare class PluginAreaModule extends NativeModule<Events> {
  warmUp(): void;
  coolDown(): void;
  createArea(areaId: string, setupJson: string): Promise<void>;
  destroyArea(areaId: string): void;
  destroyAllAreas(): void;
  setSetup(areaId: string, setupJson: string): void;
  pushBatch(areaId: string, json: string): void;
  hostReply(areaId: string, id: number, json: string): void;
  back(areaId: string): Promise<boolean>;
}

const native = requireOptionalNativeModule<PluginAreaModule>('PluginArea');

/** False when the native module isn't in the build. */
export const available: boolean = native != null;

const none: Subscription = { remove() {} };

/** Makes the engine group and one spare engine, so the next area starts fast. */
export function warmUp(): void {
  native?.warmUp();
}

/** Drops the spare engine. */
export function coolDown(): void {
  native?.coolDown();
}

/** Starts an area; resolves when its Dart side is listening. */
export async function createArea(areaId: string, setupJson: string): Promise<void> {
  if (!native) throw new Error('unavailable');
  await native.createArea(areaId, setupJson);
}

export function destroyArea(areaId: string): void {
  native?.destroyArea(areaId);
}

/** Destroys every area, as an earlier JavaScript start may have left some. */
export function destroyAllAreas(): void {
  native?.destroyAllAreas();
}

export function setSetup(areaId: string, setupJson: string): void {
  native?.setSetup(areaId, setupJson);
}

/** One batch, exactly as the core gave it; held natively until Dart is ready. */
export function pushBatch(areaId: string, json: string): void {
  native?.pushBatch(areaId, json);
}

/** The answer to a host call: `{"ok":…}` or `{"error":"<kind>"}`. */
export function hostReply(areaId: string, id: number, json: string): void {
  native?.hostReply(areaId, id, json);
}

/** The system back: true when the area closed a modal, a sheet or a dialog. */
export async function back(areaId: string): Promise<boolean> {
  if (!native) return false;
  return native.back(areaId);
}

export function addInputListener(listener: (event: AreaEvent) => void): Subscription {
  return native ? native.addListener('input', listener) : none;
}

export function addStatusListener(listener: (event: AreaEvent) => void): Subscription {
  return native ? native.addListener('status', listener) : none;
}

export function addHostCallListener(listener: (event: HostCallEvent) => void): Subscription {
  return native ? native.addListener('hostCall', listener) : none;
}

export { PluginAreaView } from './PluginAreaView';
export type { AreaEvent, HostCallEvent, PluginAreaViewProps, Subscription } from './PluginArea.types';
