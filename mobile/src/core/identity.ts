// The identity secret lives in the system keychain (iOS) or keystore
// (Android), never anywhere else; the display name in the core's settings.
import * as SecureStore from 'expo-secure-store';

import type { CoreApi } from './api';

const SECRET_KEY = 'opencord.identity';
export const NAME_SETTING = 'mobile.displayName';

const options: SecureStore.SecureStoreOptions = {
  keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
};

export function toBase64(bytes: ArrayBuffer): string {
  let binary = '';
  for (const byte of new Uint8Array(bytes)) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export function fromBase64(text: string): ArrayBuffer {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes.buffer;
}

export interface SavedIdentity {
  secret: ArrayBuffer;
  name: string;
}

export async function loadIdentity(core: CoreApi): Promise<SavedIdentity | null> {
  const secret = await SecureStore.getItemAsync(SECRET_KEY, options);
  const name = core.settingsGet(NAME_SETTING);
  if (secret === null || name === undefined) return null;
  return { secret: fromBase64(secret), name };
}

export async function saveIdentity(core: CoreApi, secret: ArrayBuffer, name: string): Promise<void> {
  await SecureStore.setItemAsync(SECRET_KEY, toBase64(secret), options);
  core.settingsSet(NAME_SETTING, name);
}

export async function clearIdentity(core: CoreApi): Promise<void> {
  await SecureStore.deleteItemAsync(SECRET_KEY, options);
  core.settingsSet(NAME_SETTING, undefined);
}
