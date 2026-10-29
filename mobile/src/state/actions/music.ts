// Music actions: player control, playback, playlist queue, search and uploads.
import {
  type MusicAction,
  type MusicPlacement,
  type MusicRepeat,
  type MusicSearchResultsRecord,
  type MusicSessionRecord,
} from '@opencord/core-native';

import { getCore } from '@/core';
import { channelKey } from '../types';
import { useAppStore } from '../store';

/** Subscribes to live music updates for a voice channel. */
export async function subscribeMusic(serverKey: string, channelId: string): Promise<void> {
  const session = await getCore().musicSubscribe(serverKey, channelId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

/** Unsubscribes from music session updates. */
export async function unsubscribeMusic(serverKey: string, channelId: string): Promise<void> {
  await getCore().musicUnsubscribe(serverKey, channelId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => {
    const next = { ...state.music };
    delete next[key];
    return { music: next };
  });
}

/** Control playback (play, pause, next, prev, seek, stop). */
export async function playback(
  serverKey: string,
  channelId: string,
  action: MusicAction,
  positionMs?: number,
  expectedItemId?: string,
): Promise<void> {
  const session = await getCore().musicPlayback(
    serverKey,
    channelId,
    action,
    positionMs,
    expectedItemId,
  );
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

/** Set repeat and shuffle modes. */
export async function setMode(
  serverKey: string,
  channelId: string,
  repeat?: MusicRepeat,
  shuffle?: boolean,
): Promise<void> {
  const session = await getCore().musicSetMode(serverKey, channelId, repeat, shuffle);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

/** Vote to skip the current track. */
export async function voteSkip(
  serverKey: string,
  channelId: string,
  expectedItemId: string,
): Promise<void> {
  const session = await getCore().musicVoteSkip(serverKey, channelId, expectedItemId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

// Stubs for Task 19 & 20
export async function enqueue(
  serverKey: string,
  channelId: string,
  resultIds: string[],
  historyItemIds: string[],
  beforeItemId: string | undefined,
  placement: MusicPlacement,
): Promise<MusicSessionRecord> {
  const session = await getCore().musicEnqueue(
    serverKey,
    channelId,
    resultIds,
    historyItemIds,
    beforeItemId,
    placement,
  );
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
  return session;
}

export async function moveItem(
  serverKey: string,
  channelId: string,
  itemId: string,
  beforeItemId: string | undefined,
): Promise<MusicSessionRecord> {
  const session = await getCore().musicQueueMove(serverKey, channelId, itemId, beforeItemId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
  return session;
}

export async function removeItems(
  serverKey: string,
  channelId: string,
  itemIds: string[],
): Promise<MusicSessionRecord> {
  const session = await getCore().musicQueueRemove(serverKey, channelId, itemIds);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
  return session;
}

export async function clearQueue(
  serverKey: string,
  channelId: string,
): Promise<MusicSessionRecord> {
  const session = await getCore().musicQueueClear(serverKey, channelId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
  return session;
}

export async function search(
  serverKey: string,
  channelId: string,
  query: string,
  source: string,
): Promise<MusicSearchResultsRecord> {
  return await getCore().musicSearch(serverKey, channelId, query, source);
}
