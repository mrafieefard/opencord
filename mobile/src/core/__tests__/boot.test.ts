import { MobileError } from '@opencord/core-native';
import * as SecureStore from 'expo-secure-store';

import { bootCore, type BootDeps } from '../boot';
import { FakeCore } from '../fake/FakeCore';
import { fromBase64 } from '../identity';
import { PRIVATE_KEY_NAME, STARTED_FRESH, loadOrCreatePrivateKey } from '../privateKey';

const keystore = (SecureStore as unknown as { __items: Map<string, string> }).__items;
const getItem = SecureStore.getItemAsync as jest.Mock;

beforeEach(() => {
  keystore.clear();
});

/** The keystore's private key, as bytes. */
function savedKey(): number[] {
  const saved = keystore.get(PRIVATE_KEY_NAME);
  if (saved === undefined) throw new Error('no private key saved');
  return Array.from(new Uint8Array(fromBase64(saved)));
}

function openedWith(core: FakeCore): number[][] {
  return core.calls
    .filter((call) => call.method === 'privateOpen')
    .map((call) => Array.from(new Uint8Array(call.args[0] as ArrayBuffer)));
}

const deps = (identity: Awaited<ReturnType<BootDeps['loadIdentity']>>): BootDeps => ({
  dataDir: () => '/data',
  cacheDir: () => '/cache',
  loadIdentity: async () => identity,
  pluginRenderer: false,
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

  // Fails if the phone lists a plugin feature it can't draw (a server would
  // send plugin channels and activities nothing shows), or sets the
  // features after the saved servers start connecting.
  it('features follow the renderer', async () => {
    const secret = new Uint8Array(32).fill(3).buffer;
    for (const [pluginRenderer, features] of [
      [false, ['plugin_users']],
      [true, ['plugin_users', 'plugin_channels', 'activities']],
    ] as const) {
      const core = new FakeCore({ scenario: 'empty' });
      await bootCore(core, { ...deps({ secret, name: 'Ava' }), pluginRenderer });
      const methods = core.calls.map((call) => call.method);
      expect(core.calls.find((call) => call.method === 'setPluginFeatures')?.args).toEqual([features]);
      expect(methods.indexOf('setPluginFeatures')).toBeGreaterThan(methods.indexOf('coreInit'));
      expect(methods.indexOf('setPluginFeatures')).toBeLessThan(methods.indexOf('identityLoad'));
    }
  });
});

describe('private messages at start', () => {
  const secret = new Uint8Array(32).fill(3).buffer;

  // Review focus 5. Fails if the sentence shows on every start or reload,
  // never shows, or a new key is made at each start (every restart would
  // set the chats aside again).
  it('a missing master key starts fresh and says so once', async () => {
    // A database from before, sealed with a key the keystore no longer has.
    const core = new FakeCore({ scenario: 'empty' });
    core.sealPrivateDatabase(new Uint8Array(32).fill(9).buffer);

    const first = await bootCore(core, deps({ secret, name: 'Ava' }));
    expect(first).toMatchObject({ state: 'ready', notice: STARTED_FRESH });
    const methods = core.calls.map((call) => call.method);
    expect(methods.indexOf('privateOpen')).toBeGreaterThan(methods.indexOf('identityLoad'));

    // A JavaScript reload: the same core, opened again.
    const reload = await bootCore(core, deps({ secret, name: 'Ava' }));
    expect(reload).toMatchObject({ state: 'ready', notice: null });

    // The next start of the app: its database is sealed with the saved key.
    const restarted = new FakeCore({ scenario: 'empty' });
    restarted.sealPrivateDatabase(new Uint8Array(savedKey()).buffer);
    const next = await bootCore(restarted, deps({ secret, name: 'Ava' }));
    expect(next).toMatchObject({ state: 'ready', notice: null });
  });

  // Fails if the key is made again while one is saved, isn't saved, or
  // the core is opened with another key than the saved one.
  it('the key is made once and reused', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    await bootCore(core, deps({ secret, name: 'Ava' }));
    await bootCore(core, deps({ secret, name: 'Ava' }));
    const again = await loadOrCreatePrivateKey(core);

    expect(core.calls.filter((call) => call.method === 'privateKeyGenerate')).toHaveLength(1);
    const key = savedKey();
    expect(key).toHaveLength(32);
    expect(openedWith(core)).toEqual([key, key]);
    expect(Array.from(again)).toEqual(key);
  });

  // Fails if a keystore that can't be read is taken for an empty one: a
  // new key written over the old would set the chats aside for good.
  it('a keystore that fails to read keeps the saved key', async () => {
    keystore.set(PRIVATE_KEY_NAME, 'AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=');
    getItem.mockRejectedValueOnce(new Error('keystore locked'));
    const core = new FakeCore({ scenario: 'empty' });

    const result = await bootCore(core, deps({ secret, name: 'Ava' }));

    expect(result).toMatchObject({ state: 'ready', notice: "Private messages couldn't be opened. Something went wrong." });
    expect(core.calls.some((call) => call.method === 'privateKeyGenerate')).toBe(false);
    expect(core.calls.some((call) => call.method === 'privateOpen')).toBe(false);
    expect(savedKey()).toEqual(new Array(32).fill(1));
  });

  // Fails if private messages failing to open fails the start: the
  // identity and the servers don't depend on them.
  it('private messages failing to open still starts', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    core.failNext('privateOpen', new MobileError.Storage({ message: 'disk full' }));

    const result = await bootCore(core, deps({ secret, name: 'Ava' }));

    expect(result).toMatchObject({
      state: 'ready',
      self: { displayName: 'Ava' },
      notice: "Private messages couldn't be opened. Couldn't save on this phone. It may be out of space.",
    });
  });
});
