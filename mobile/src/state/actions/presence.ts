// The person's presence and profile picture, synchronized across servers (spec §6.2, §6.3).
import { ConnectionRecord, MobileError, PresenceStatus } from '@opencord/core-native';

import { getCore } from '@/core';
import { describeError } from '@/core/errors';

import { useAppStore } from '../store';
import type { UserStatus } from '../types';
import { STATUS } from './prefs';
import { toast } from './ui';

export function toPresenceStatus(status: UserStatus): PresenceStatus {
  switch (status) {
    case 'online':
      return PresenceStatus.Online;
    case 'idle':
      return PresenceStatus.Idle;
    case 'dnd':
      return PresenceStatus.Dnd;
  }
}

/** Stores a chosen status and sends it to every connected server. */
export async function setStatus(status: UserStatus): Promise<void> {
  const core = getCore();
  core.settingsSet(STATUS, status);
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, status } }));

  const state = useAppStore.getState();
  const connected = Object.values(state.servers).filter((server) =>
    ConnectionRecord.Connected.instanceOf(server.connection),
  );

  const presence = toPresenceStatus(status);
  await Promise.all(
    connected.map((server) =>
      core.presenceUpdate(server.key, presence).catch((error: unknown) => {
        if (!MobileError.NotConnected.instanceOf(error)) {
          console.warn(`presence update failed for ${server.key}: ${describeError(error)}`);
        }
      }),
    ),
  );
}

/** After a server's Ready: sends the current presence status to it. */
export async function catchUpPresence(serverKey: string): Promise<void> {
  const status = useAppStore.getState().prefs.status;
  const presence = toPresenceStatus(status);
  try {
    await getCore().presenceUpdate(serverKey, presence);
  } catch (error: unknown) {
    if (!MobileError.NotConnected.instanceOf(error)) {
      console.warn(`catch-up presence failed for ${serverKey}: ${describeError(error)}`);
    }
  }
}

/** After a server's Ready: if the server knows an old picture, uploads the current avatar. */
export async function catchUpAvatar(serverKey: string): Promise<void> {
  const state = useAppStore.getState();
  const server = state.servers[serverKey];
  const snapshot = server?.snapshot;
  if (!snapshot) return;

  const core = getCore();
  const ownAvatar = core.ownAvatar();
  if (snapshot.selfUser.avatarHash === ownAvatar.hash) return;

  try {
    await core.avatarUpload(serverKey);
  } catch (error: unknown) {
    if (!MobileError.NotConnected.instanceOf(error)) {
      const name = snapshot.name ?? server.entry.name;
      toast(`${name} kept your old picture: ${describeError(error)}`);
    }
  }
}

/**
 * Sets a new profile picture (or removes it with undefined), then uploads to
 * all servers concurrently. NotConnected is ignored (Ready catches up).
 * The first other refusal shows a toast naming its server.
 */
export async function setAvatar(sourcePath: string | undefined): Promise<void> {
  const core = getCore();
  const newHash = core.avatarSet(sourcePath);

  const self = useAppStore.getState().self;
  if (self) {
    useAppStore.setState({ self: { ...self, avatarHash: newHash ?? undefined } });
  }

  const servers = Object.values(useAppStore.getState().servers);
  let firstRefusal: { name: string; error: unknown } | null = null;

  await Promise.all(
    servers.map(async (server) => {
      try {
        await core.avatarUpload(server.key);
      } catch (error: unknown) {
        if (MobileError.NotConnected.instanceOf(error)) {
          // Servers not connected now get it after their next Ready.
          return;
        }
        if (!firstRefusal) {
          const name = server.snapshot?.name ?? server.entry.name;
          firstRefusal = { name, error };
        }
      }
    }),
  );

  if (firstRefusal) {
    const { name, error } = firstRefusal;
    toast(`${name} kept your old picture: ${describeError(error)}`);
  }
}
