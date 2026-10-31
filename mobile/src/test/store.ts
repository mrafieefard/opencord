// A fresh store over a FakeCore for each test.
import { setCore } from '@/core';
import { FakeCore, type FakeOptions } from '@/core/fake/FakeCore';
import { loadServers } from '@/state/actions/servers';
import { initialState, readPositions, useAppStore } from '@/state/store';

export function freshStore(options: FakeOptions = {}): FakeCore {
  const core = new FakeCore({ now: () => 1_761_500_000_000, ...options });
  setCore(core);
  readPositions.reset();
  useAppStore.setState(initialState(), true);
  useAppStore.setState({ boot: { state: 'ready' }, self: { displayName: 'Ava', fingerprint: 'F', publicKeyHex: 'p' } });
  loadServers();
  return core;
}

/** Lets promises settle (seeding's fetches). */
export async function settle(): Promise<void> {
  for (let round = 0; round < 10; round += 1) await new Promise<void>((resolve) => setImmediate(resolve));
}
