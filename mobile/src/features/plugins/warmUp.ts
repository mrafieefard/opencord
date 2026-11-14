// Warming the plugin engine before it's needed (plugins M4 plan, Task 13):
// a spare engine is made while the open server has plugin channel types or
// activities, or while this device is in a call, and dropped 60 seconds
// after neither holds. Nothing happens in a build without the renderer.
import * as PluginArea from '@opencord/plugin-area';
import { useEffect } from 'react';

import { useAppStore } from '@/state/store';
import { useServer } from '@/state/selectors';

export const COOL_DOWN_MS = 60_000;

/**
 * The reasons to keep a spare engine, each counted (two screens of one
 * server can be mounted during a transition), and the timer that drops it.
 */
class Warmth {
  private readonly reasons = new Map<string, number>();
  private warm = false;
  private coolTimer: ReturnType<typeof setTimeout> | null = null;

  hold(reason: string): void {
    if (!PluginArea.available) return;
    this.reasons.set(reason, (this.reasons.get(reason) ?? 0) + 1);
    if (this.coolTimer !== null) {
      clearTimeout(this.coolTimer);
      this.coolTimer = null;
    }
    if (!this.warm) {
      this.warm = true;
      PluginArea.warmUp();
    }
  }

  release(reason: string): void {
    const holders = this.reasons.get(reason);
    if (holders === undefined) return;
    if (holders > 1) {
      this.reasons.set(reason, holders - 1);
      return;
    }
    this.reasons.delete(reason);
    if (this.reasons.size > 0 || !this.warm) return;
    if (this.coolTimer !== null) clearTimeout(this.coolTimer);
    this.coolTimer = setTimeout(() => {
      this.coolTimer = null;
      if (this.reasons.size > 0) return;
      this.warm = false;
      PluginArea.coolDown();
    }, COOL_DOWN_MS);
  }

  /** Forgets everything (tests). */
  reset(): void {
    if (this.coolTimer !== null) clearTimeout(this.coolTimer);
    this.coolTimer = null;
    this.reasons.clear();
    this.warm = false;
  }
}

export const warmth = new Warmth();

/** Keeps the engine warm while `serverKey`'s server has plugins. */
export function useEngineWarmUp(serverKey: string): void {
  const snapshot = useServer(serverKey)?.snapshot;
  const hasPlugins = (snapshot?.pluginChannelTypes.length ?? 0) > 0 || (snapshot?.pluginActivities.length ?? 0) > 0;
  useEffect(() => {
    if (!hasPlugins) return undefined;
    const reason = `server:${serverKey}`;
    warmth.hold(reason);
    return () => warmth.release(reason);
  }, [hasPlugins, serverKey]);
}

/** Keeps the engine warm while this device is in a call; returns the stop. */
export function watchCallWarmth(): () => void {
  const follow = (inCall: boolean) => (inCall ? warmth.hold('call') : warmth.release('call'));
  let inCall = useAppStore.getState().myVoice !== null;
  if (inCall) follow(true);
  const unsubscribe = useAppStore.subscribe((state) => {
    const now = state.myVoice !== null;
    if (now === inCall) return;
    inCall = now;
    follow(now);
  });
  return () => {
    unsubscribe();
    warmth.release('call');
  };
}
