import { MobileError } from '@opencord/core-native';

import { bootCore, type BootDeps } from '../boot';
import { FakeCore } from '../fake/FakeCore';

const deps = (identity: Awaited<ReturnType<BootDeps['loadIdentity']>>): BootDeps => ({
  dataDir: () => '/data',
  cacheDir: () => '/cache',
  loadIdentity: async () => identity,
});

describe('starting the core', () => {
  it('no identity goes to onboarding', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    expect(await bootCore(core, deps(null))).toEqual({ state: 'onboarding' });
    expect(core.calls[0]).toEqual({ method: 'coreInit', args: ['/data', '/cache'] });
  });

  it('a saved identity loads and goes to ready', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const secret = new Uint8Array(32).fill(3).buffer;
    const result = await bootCore(core, deps({ secret, name: 'Ava' }));
    expect(result).toMatchObject({ state: 'ready', self: { displayName: 'Ava' } });
    expect(core.calls.find((call) => call.method === 'identityLoad')?.args).toEqual([secret, 'Ava']);
  });

  it('a core that fails to open reports its sentence', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    core.failNext('coreInit', new MobileError.Storage({ message: 'disk full' }));
    expect(await bootCore(core, deps(null))).toEqual({
      state: 'failed',
      message: "Couldn't save on this phone. It may be out of space.",
    });
  });
});
