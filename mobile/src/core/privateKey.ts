// The private messages database's master key: 32 random bytes in the system
// keychain (iOS) or keystore (Android), under the desktop's name for it
// (app/lib/core/rust/private_key_store.dart). Losing it loses this device's
// chat history, never the identity: the core then starts fresh.
import * as SecureStore from 'expo-secure-store';

import type { CoreApi } from './api';
import { fromBase64, toBase64 } from './identity';

export const PRIVATE_KEY_NAME = 'opencord.private.key';
export const PRIVATE_KEY_BYTES = 32;

/** What the desktop says when its private database couldn't be opened. */
export const STARTED_FRESH =
  "Your private messages on this device couldn't be opened, so this device starts fresh. New messages will arrive normally.";

// Never backed up in plain text: this device only, readable after the first
// unlock (iOS); expo-secure-store keeps Android's in encrypted preferences.
const options: SecureStore.SecureStoreOptions = {
  keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
};

const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

/** The saved key, or null when the entry isn't a 32-byte key. */
function decode(saved: string): Uint8Array<ArrayBuffer> | null {
  if (!BASE64.test(saved)) return null;
  const key = new Uint8Array(fromBase64(saved));
  return key.length === PRIVATE_KEY_BYTES ? key : null;
}

/**
 * The saved master key, or a new one from the core's random source, saved
 * before it is used. A keystore that fails to read throws: a new key written
 * over a passing failure would make the core set the chats aside for good.
 */
export async function loadOrCreatePrivateKey(core: CoreApi): Promise<Uint8Array<ArrayBuffer>> {
  const saved = await SecureStore.getItemAsync(PRIVATE_KEY_NAME, options);
  if (saved !== null) {
    const key = decode(saved);
    if (key !== null) return key;
    // As the desktop: an entry that isn't a key is replaced.
    console.warn('the private key entry is not a key; making a new one');
  }
  const key = new Uint8Array(core.privateKeyGenerate());
  if (key.length !== PRIVATE_KEY_BYTES) throw new Error(`a new private key has ${key.length} bytes, not ${PRIVATE_KEY_BYTES}`);
  await SecureStore.setItemAsync(PRIVATE_KEY_NAME, toBase64(key.buffer), options);
  return key;
}

/**
 * Opens the private database with the keystore's key, after the identity.
 * Returns the sentence to show once when the core had to start fresh.
 */
export async function openPrivate(core: CoreApi): Promise<string | undefined> {
  const key = await loadOrCreatePrivateKey(core);
  const opened = await core.privateOpen(key.buffer);
  return opened.startedFresh ? STARTED_FRESH : undefined;
}
