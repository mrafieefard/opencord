// Music actions: player control, playback, playlist queue, search and uploads.
import {
  UploadPhaseRecord,
  type MusicAction,
  type MusicPlacement,
  type MusicRepeat,
  type MusicSearchResultRecord,
} from '@opencord/core-native';
import { File } from 'expo-file-system';

import { getCore } from '@/core';
import { fileUri } from '@/core/paths';
import { channelKey, type PickedFile } from '../types';
import { useAppStore } from '../store';

/** Fetches a server's music settings into the store; MusicSettingsChanged keeps them current after. */
export async function loadMusicSettings(serverKey: string): Promise<void> {
  const settings = await getCore().musicSettings(serverKey);
  useAppStore.setState((state) => {
    const server = state.servers[serverKey];
    return server ? { servers: { ...state.servers, [serverKey]: { ...server, musicSettings: settings } } } : state;
  });
}

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

export async function enqueue(
  serverKey: string,
  channelId: string,
  resultIds: string[],
  historyItemIds: string[],
  beforeItemId: string | undefined,
  placement: MusicPlacement,
): Promise<void> {
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
}

export async function moveItem(
  serverKey: string,
  channelId: string,
  itemId: string,
  beforeItemId: string | undefined,
): Promise<void> {
  const session = await getCore().musicQueueMove(serverKey, channelId, itemId, beforeItemId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

export async function removeItems(
  serverKey: string,
  channelId: string,
  itemIds: string[],
): Promise<void> {
  const session = await getCore().musicQueueRemove(serverKey, channelId, itemIds);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

export async function clearQueue(
  serverKey: string,
  channelId: string,
): Promise<void> {
  const session = await getCore().musicQueueClear(serverKey, channelId);
  const key = channelKey(serverKey, channelId);
  useAppStore.setState((state) => ({
    music: { ...state.music, [key]: session },
  }));
}

export async function search(
  serverKey: string,
  channelId: string,
  query: string,
  source: string,
): Promise<MusicSearchResultRecord[]> {
  const res = await getCore().musicSearch(serverKey, channelId, query, source);
  return res.results;
}

function deleteCopy(path: string): void {
  if (!path) return;
  try {
    const file = new File(fileUri(path));
    file.delete();
  } catch {
    // Harmless if missing or already deleted
  }
}

let nextUploadId = 1;

export async function uploadSongs(
  serverKey: string,
  channelId: string,
  files: PickedFile[],
): Promise<void> {
  const core = getCore();
  for (const file of files) {
    const uploadId = `u-${Date.now()}-${nextUploadId++}`;
    useAppStore.setState((state) => ({
      uploads: {
        ...state.uploads,
        [uploadId]: {
          uploadId,
          serverKey,
          channelId,
          filename: file.filename,
          sentBytes: 0,
          totalBytes: file.size,
          phase: new UploadPhaseRecord.Preparing(),
          path: file.path,
        },
      },
    }));

    // Every file uploads at once
    core
      .musicUploadSong(serverKey, channelId, uploadId, file.path, false)
      .catch(() => {
        // Handled via core events
      });
  }
}

export function cancelUpload(uploadId: string): void {
  const upload = useAppStore.getState().uploads[uploadId];
  if (!upload) return;
  getCore().musicCancelSong(upload.serverKey, uploadId);
  deleteCopy(upload.path);
}

export async function retryUpload(uploadId: string): Promise<void> {
  const upload = useAppStore.getState().uploads[uploadId];
  if (!upload) return;
  useAppStore.setState((state) => ({
    uploads: {
      ...state.uploads,
      [uploadId]: {
        ...upload,
        sentBytes: 0,
        phase: new UploadPhaseRecord.Preparing(),
      },
    },
  }));
  getCore()
    .musicUploadSong(upload.serverKey, upload.channelId, uploadId, upload.path, false)
    .catch(() => {
      // Handled via core events
    });
}

export function dismissUpload(uploadId: string): void {
  const upload = useAppStore.getState().uploads[uploadId];
  if (upload) {
    deleteCopy(upload.path);
  }
  useAppStore.setState((state) => {
    const next = { ...state.uploads };
    delete next[uploadId];
    return { uploads: next };
  });
}

