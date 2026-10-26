// Making an identity, or bringing one from a desktop backup (design §8.1).
import { getCore } from '@/core';
import { startApp } from '@/core/lifecycle';
import { saveIdentity } from '@/core/identity';
import { useAppStore } from '@/state/store';

/** Generates an identity, saves it, loads it. Throws the core's sentence. */
export async function createIdentity(name: string, secret?: ArrayBuffer): Promise<void> {
  const core = getCore();
  const chosen = secret ?? core.identityGenerate().secret;
  const info = core.identityCheck(chosen, name);
  const trimmed = name.trim();
  await saveIdentity(core, chosen, trimmed);
  core.identityLoad(chosen, trimmed);
  useAppStore.setState({
    boot: { state: 'ready' },
    self: { displayName: trimmed, fingerprint: info.fingerprint, publicKeyHex: info.publicKeyHex },
  });
  startApp();
}

/** The secret in a backup file's text; throws when it isn't one. */
export function readBackup(text: string): ArrayBuffer {
  return getCore().identityBackupDecode(text);
}
