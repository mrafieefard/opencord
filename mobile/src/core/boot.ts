// Opening the core at start: its data, then the saved identity (design
// §8.1), then private messages. Without an identity, onboarding; with one,
// the servers.
import type { CoreApi } from './api';
import { describeError, errorKind } from './errors';
import type { SavedIdentity } from './identity';
import { openPrivate } from './privateKey';

export interface SelfInfo {
  displayName: string;
  fingerprint: string;
  publicKeyHex: string;
  avatarHash?: string;
}

export type BootResult =
  | { state: 'onboarding' }
  /** `notice`: a sentence to show once the app is up. */
  | { state: 'ready'; self: SelfInfo; notice: string | null }
  | { state: 'failed'; message: string };

export interface BootDeps {
  dataDir: () => string;
  cacheDir: () => string;
  loadIdentity: (core: CoreApi) => Promise<SavedIdentity | null>;
  /** Whether this build can draw plugin areas (`@opencord/plugin-area`). */
  pluginRenderer: boolean;
}

/** What the phone tells servers it can show: only what it can draw. */
export function pluginFeatures(pluginRenderer: boolean): string[] {
  return pluginRenderer ? ['plugin_users', 'plugin_channels', 'activities'] : ['plugin_users'];
}

export async function bootCore(core: CoreApi, deps: BootDeps): Promise<BootResult> {
  try {
    core.coreInit(deps.dataDir(), deps.cacheDir());
    // Before any server connects: Identify lists them.
    core.setPluginFeatures(pluginFeatures(deps.pluginRenderer));
    const saved = await deps.loadIdentity(core);
    if (saved === null) return { state: 'onboarding' };
    const info = core.identityLoad(saved.secret, saved.name);
    return {
      state: 'ready',
      self: { displayName: saved.name, fingerprint: info.fingerprint, publicKeyHex: info.publicKeyHex },
      notice: await openPrivateMessages(core),
    };
  } catch (error: unknown) {
    console.warn(`start failed: ${errorKind(error)}`);
    return { state: 'failed', message: describeError(error) };
  }
}

/**
 * Opens private messages once the identity is loaded: at start, and after
 * onboarding. Returns what to show: the fresh-start sentence, or why they
 * couldn't open. Their failure never stops the app starting: losing them
 * never costs the identity.
 */
export async function openPrivateMessages(core: CoreApi): Promise<string | null> {
  try {
    return (await openPrivate(core)) ?? null;
  } catch (error: unknown) {
    console.warn(`private messages failed to open: ${errorKind(error)}`);
    return `Private messages couldn't be opened. ${describeError(error)}`;
  }
}
