// How far each channel was read, kept on this phone in the core's settings
// (`mobile.read.<serverKey>`, channel id → message id). Positions only move
// forward; writes go out at most once per 2 s per server.
import type { CoreApi } from '@/core/api';

import { compareIds } from './types';

const WRITE_EVERY_MS = 2000;

export class ReadPositions {
  private cache = new Map<string, Record<string, string>>();
  private pending = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(private readonly core: () => CoreApi) {}

  private settingKey(serverKey: string): string {
    return `mobile.read.${serverKey}`;
  }

  private load(serverKey: string): Record<string, string> {
    const cached = this.cache.get(serverKey);
    if (cached) return cached;
    let positions: Record<string, string> = {};
    const text = this.core().settingsGet(this.settingKey(serverKey));
    if (text !== undefined) {
      try {
        const parsed: unknown = JSON.parse(text);
        if (parsed && typeof parsed === 'object') {
          positions = Object.fromEntries(
            Object.entries(parsed).filter((entry): entry is [string, string] => typeof entry[1] === 'string'),
          );
        }
      } catch {
        // A broken setting starts over: every channel counts as never read.
        console.warn('read positions could not be parsed; starting over');
      }
    }
    this.cache.set(serverKey, positions);
    return positions;
  }

  of(serverKey: string, channelId: string): string | undefined {
    return this.load(serverKey)[channelId];
  }

  /** Read up to `messageId`; never moves back. */
  save(serverKey: string, channelId: string, messageId: string): void {
    const positions = this.load(serverKey);
    const current = positions[channelId];
    if (current !== undefined && compareIds(current, messageId) >= 0) return;
    this.cache.set(serverKey, { ...positions, [channelId]: messageId });
    this.schedule(serverKey);
  }

  /** A channel never seen, empty: what arrives from now on counts. */
  start(serverKey: string, channelId: string): void {
    if (this.of(serverKey, channelId) !== undefined) return;
    this.cache.set(serverKey, { ...this.load(serverKey), [channelId]: '0' });
    this.schedule(serverKey);
  }

  /** Tests start from nothing. */
  reset(): void {
    for (const timer of this.pending.values()) clearTimeout(timer);
    this.pending.clear();
    this.cache.clear();
  }

  forget(serverKey: string): void {
    this.cache.delete(serverKey);
    const timer = this.pending.get(serverKey);
    if (timer) clearTimeout(timer);
    this.pending.delete(serverKey);
    this.core().settingsSet(this.settingKey(serverKey), undefined);
  }

  private schedule(serverKey: string): void {
    if (this.pending.has(serverKey)) return;
    this.pending.set(
      serverKey,
      setTimeout(() => {
        this.pending.delete(serverKey);
        this.core().settingsSet(this.settingKey(serverKey), JSON.stringify(this.load(serverKey)));
      }, WRITE_EVERY_MS),
    );
  }
}
