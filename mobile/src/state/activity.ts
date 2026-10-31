// Unread counts as the desktop works them out (rust_repository.dart
// `_recentActivity`): after each Ready, the newest 50 messages of every
// channel the user may read, 4 fetches at once, each within 10 s.
import { ChannelKind, type ServerSnapshot } from '@opencord/core-native';

import type { CoreApi } from '@/core/api';
import { errorKind } from '@/core/errors';

import type { ReadPositions } from './readPositions';
import { compareIds, type ChannelActivity } from './types';

export const SEED_MESSAGES = 50;
export const SEED_AT_ONCE = 4;
export const SEED_TIMEOUT_MS = 10_000;

function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('timed out')), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

/** The channels of `snapshot` with a chat the user may read. */
export function readableChannels(snapshot: ServerSnapshot): string[] {
  const readable = new Set(
    snapshot.access.filter((access) => access.view && access.readHistory).map((access) => access.channelId),
  );
  return snapshot.channels
    .filter((channel) => channel.kind !== ChannelKind.Category && readable.has(channel.id))
    .map((channel) => channel.id);
}

export async function seedActivity(
  core: CoreApi,
  snapshot: ServerSnapshot,
  positions: ReadPositions,
): Promise<Record<string, ChannelActivity>> {
  const serverKey = snapshot.serverKey;
  const selfId = snapshot.selfUser.id;
  const pending = readableChannels(snapshot);
  const activity: Record<string, ChannelActivity> = {};

  async function seed(channelId: string): Promise<void> {
    let recent;
    try {
      recent = await timeout(core.messagesFetch(serverKey, channelId, undefined, SEED_MESSAGES), SEED_TIMEOUT_MS);
    } catch (error: unknown) {
      console.warn(`no history for ${channelId}: ${errorKind(error)}`);
      return;
    }
    const newest = recent[0];
    if (!newest) {
      positions.start(serverKey, channelId);
      return;
    }
    let position = positions.of(serverKey, channelId);
    if (position === undefined) {
      position = newest.id;
      positions.save(serverKey, channelId, position);
    }
    const read = position;
    const unseen = recent.filter((message) => compareIds(message.id, read) > 0 && message.authorId !== selfId);
    activity[channelId] = {
      last: newest,
      lastReadId: read,
      unread: unseen.length,
      mentions: unseen.filter((message) => message.content.includes(`<@${selfId}>`)).length,
    };
  }

  await Promise.all(
    Array.from({ length: SEED_AT_ONCE }, async () => {
      for (let channelId = pending.pop(); channelId !== undefined; channelId = pending.pop()) {
        await seed(channelId);
      }
    }),
  );
  return activity;
}
