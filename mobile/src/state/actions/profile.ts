// The display name lives on this phone; servers are told it (design §6.5).
import { ConnectionRecord_Tags } from '@opencord/core-native';

import { getCore, report } from '@/core';
import { NAME_SETTING } from '@/core/identity';

import { useAppStore } from '../store';

/** Saves a new name and sends it to every connected server. */
export async function setDisplayName(name: string): Promise<void> {
  const core = getCore();
  const self = useAppStore.getState().self;
  if (!self) return;
  // Checked as the core checks it; throws its sentence.
  const trimmed = name.trim();
  core.identityCheck(new Uint8Array(32).buffer, trimmed);
  core.settingsSet(NAME_SETTING, trimmed);
  useAppStore.setState({ self: { ...self, displayName: trimmed } });
  const connected = Object.values(useAppStore.getState().servers).filter(
    (server) => server.connection.tag === ConnectionRecord_Tags.Connected,
  );
  await Promise.all(connected.map((server) => core.profileUpdate(server.key, trimmed)));
}

/** After a Ready: a server that knows this person by another name hears the saved one. */
export function catchUpName(serverKey: string): void {
  const state = useAppStore.getState();
  const name = state.self?.displayName;
  const known = state.servers[serverKey]?.snapshot?.selfUser.displayName;
  if (!name || !known || known === name) return;
  report(getCore().profileUpdate(serverKey, name), 'sending the display name');
}
