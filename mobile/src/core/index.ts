// The core this app runs on: NativeCore, or FakeCore with
// EXPO_PUBLIC_FAKE_CORE=1 (screenshots, and running without a server).
import type { CoreApi } from './api';
import type { BootDeps } from './boot';
import { FakeCore, type Scenario } from './fake/FakeCore';
import { loadIdentity } from './identity';
import { NativeCore } from './native';
import { cacheDir, dataDir } from './paths';

export const fakeMode = process.env.EXPO_PUBLIC_FAKE_CORE === '1';

let current: CoreApi | null = null;

export function getCore(): CoreApi {
  if (current === null) {
    const scenario: Scenario = process.env.EXPO_PUBLIC_FAKE_SCENARIO === 'empty' ? 'empty' : 'default';
    current = fakeMode ? new FakeCore({ scenario, live: true }) : NativeCore;
  }
  return current;
}

/** Tests put their own core in place. */
export function setCore(core: CoreApi): void {
  current = core;
}

export function bootDeps(): BootDeps {
  const core = getCore();
  if (core instanceof FakeCore && core.scenario === 'default') {
    return { dataDir: () => '/fake/data', cacheDir: () => '/fake/cache', loadIdentity: async () => core.fakeIdentity() };
  }
  return { dataDir, cacheDir, loadIdentity };
}

export type { CoreApi } from './api';
export { bootCore, type BootResult, type SelfInfo } from './boot';
export { describeError, errorKind, failureSentence } from './errors';
export { startEventLoop, type EventLoop } from './eventLoop';
export { report, setErrorShower } from './report';
