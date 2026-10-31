// Opening the core at start: its data, then the saved identity (design
// §8.1). Without one, onboarding; with one, the servers.
import type { CoreApi } from './api';
import { describeError, errorKind } from './errors';
import type { SavedIdentity } from './identity';

export interface SelfInfo {
  displayName: string;
  fingerprint: string;
  publicKeyHex: string;
}

export type BootResult =
  | { state: 'onboarding' }
  | { state: 'ready'; self: SelfInfo }
  | { state: 'failed'; message: string };

export interface BootDeps {
  dataDir: () => string;
  cacheDir: () => string;
  loadIdentity: (core: CoreApi) => Promise<SavedIdentity | null>;
}

export async function bootCore(core: CoreApi, deps: BootDeps): Promise<BootResult> {
  try {
    core.coreInit(deps.dataDir(), deps.cacheDir());
    const saved = await deps.loadIdentity(core);
    if (saved === null) return { state: 'onboarding' };
    const info = core.identityLoad(saved.secret, saved.name);
    return {
      state: 'ready',
      self: { displayName: saved.name, fingerprint: info.fingerprint, publicKeyHex: info.publicKeyHex },
    };
  } catch (error: unknown) {
    console.warn(`start failed: ${errorKind(error)}`);
    return { state: 'failed', message: describeError(error) };
  }
}
