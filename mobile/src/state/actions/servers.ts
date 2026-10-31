// The server list: loading it, adding, trusting, retrying, leaving, and
// marking read.
import { AddServerResult, ConnectionRecord_Tags } from '@opencord/core-native';

import { getCore } from '@/core';

import { newServer } from '../events';
import { readPositions, useAppStore } from '../store';
import { compareIds } from '../types';

export function loadServers(): void {
  const entries = getCore().serversList();
  useAppStore.setState((state) => {
    const servers = { ...state.servers };
    for (const entry of entries) {
      const current = servers[entry.key];
      servers[entry.key] = current ? { ...current, entry } : newServer(entry);
    }
    return { servers, order: entries.map((entry) => entry.key) };
  });
}

export async function addServer(linkOrAddress: string, claimToken?: string): Promise<AddServerResult> {
  const result = await getCore().serverAdd(linkOrAddress.trim(), claimToken?.trim() || undefined);
  if (AddServerResult.Added.instanceOf(result)) {
    const entry = result.inner.server;
    useAppStore.setState((state) => ({
      servers: state.servers[entry.key] ? state.servers : { ...state.servers, [entry.key]: newServer(entry) },
      order: state.order.includes(entry.key) ? state.order : [...state.order, entry.key],
    }));
  }
  return result;
}

export function trustServer(address: string, fingerprint: string): void {
  getCore().serverTrust(address, fingerprint);
}

export async function retryServer(serverKey: string): Promise<void> {
  await getCore().serverRetryNow(serverKey);
}

/** Every server that isn't connected, at once (pull to refresh, coming back). */
export async function retryDisconnected(): Promise<void> {
  const servers = Object.values(useAppStore.getState().servers);
  await Promise.all(
    servers
      .filter((server) => server.connection.tag !== ConnectionRecord_Tags.Connected)
      .map((server) =>
        getCore()
          .serverRetryNow(server.key)
          .catch((error: unknown) => console.warn(`retry ${server.key} failed`, error)),
      ),
  );
}

export async function leaveServer(serverKey: string): Promise<void> {
  await getCore().serverRemove(serverKey);
  readPositions.forget(serverKey);
  useAppStore.setState((state) => {
    const { [serverKey]: _gone, ...servers } = state.servers;
    const messages = Object.fromEntries(
      Object.entries(state.messages).filter(([key]) => !key.startsWith(`${serverKey}/`)),
    );
    return { servers, messages, order: state.order.filter((key) => key !== serverKey) };
  });
}

/** Read up to `messageId` in one channel: the count goes to 0. */
export function markChannelRead(serverKey: string, channelId: string, messageId: string): void {
  readPositions.save(serverKey, channelId, messageId);
  useAppStore.setState((state) => {
    const server = state.servers[serverKey];
    const activity = server?.activity[channelId];
    if (!server || !activity) return state;
    const lastReadId = compareIds(messageId, activity.lastReadId) > 0 ? messageId : activity.lastReadId;
    if (activity.unread === 0 && activity.mentions === 0 && lastReadId === activity.lastReadId) return state;
    return {
      servers: {
        ...state.servers,
        [serverKey]: {
          ...server,
          activity: { ...server.activity, [channelId]: { ...activity, lastReadId, unread: 0, mentions: 0 } },
        },
      },
    };
  });
}

export function markServerRead(serverKey: string): void {
  const server = useAppStore.getState().servers[serverKey];
  if (!server) return;
  for (const [channelId, activity] of Object.entries(server.activity)) {
    if (activity.last) markChannelRead(serverKey, channelId, activity.last.id);
  }
}
