import { cleanup, renderHook } from '@testing-library/react-native';
import { MobileEvent, VoiceCueKind, type VoiceStateRecord } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot, user } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import { useVoiceCues } from '../useVoiceCues';
import type { FakeCore } from '@/core/fake/FakeCore';

function makeVoiceState(params: Partial<VoiceStateRecord> & { userId: string }): VoiceStateRecord {
  return {
    channelId: undefined,
    thisDevice: false,
    selfMute: false,
    selfDeaf: false,
    serverMute: false,
    serverDeaf: false,
    ...params,
  };
}

describe('useVoiceCues', () => {
  let core: FakeCore;
  let unmountHook: (() => Promise<void>) | null = null;

  beforeEach(() => {
    core = freshStore();
    unmountHook = null;
  });

  afterEach(async () => {
    if (unmountHook) {
      await unmountHook();
      unmountHook = null;
    }
    await cleanup();
  });

  // Fails if joining a channel with people in it plays UserJoin cues instead of [Join] only.
  it('joining a channel with two people in it plays Join only', async () => {
    unmountHook = (await renderHook(() => useVoiceCues())).unmount;

    // Initial server setup with 2 other people in lounge
    const snap = snapshot(NIGHT_OWLS);
    const selfUser = user(SELF_ID);
    onEvents([
      new MobileEvent.Ready({
        snapshot: {
          ...snap,
          selfUser,
          voiceStates: [
            makeVoiceState({ userId: 'user2', channelId: CHANNELS.lounge }),
            makeVoiceState({ userId: 'user3', channelId: CHANNELS.lounge }),
          ],
        },
      }),
    ]);
    await settle();

    core.calls.length = 0;

    // Self joins lounge
    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
        }),
      }),
    ]);
    await settle();

    const cues = core.calls
      .filter((c) => c.method === 'cuePlay')
      .map((c) => c.args[0]);
    expect(cues).toEqual([VoiceCueKind.Join]);
  });

  // Fails if someone joining a different channel plays a voice cue.
  it('someone joining another channel plays nothing', async () => {
    const snap = snapshot(NIGHT_OWLS);
    const selfUser = user(SELF_ID);
    // Initial server setup
    onEvents([
      new MobileEvent.Ready({
        snapshot: {
          ...snap,
          selfUser,
          voiceStates: [],
        },
      }),
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
        }),
      }),
    ]);
    await settle();

    unmountHook = (await renderHook(() => useVoiceCues())).unmount;
    core.calls.length = 0;

    // user2 joins quiet room (different channel)
    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: 'user2',
          channelId: CHANNELS.quiet,
        }),
      }),
    ]);
    await settle();

    const cues = core.calls.filter((c) => c.method === 'cuePlay');
    expect(cues).toHaveLength(0);
  });

  // Fails if a deafen arriving as two separate updates (self mute, then self deaf) plays Mute before Deafen.
  it('deafen arriving as two updates plays only Deafen', async () => {
    const snap = snapshot(NIGHT_OWLS);
    const selfUser = user(SELF_ID);
    // Initial server setup: self in lounge unmuted
    onEvents([
      new MobileEvent.Ready({
        snapshot: {
          ...snap,
          selfUser,
          voiceStates: [],
        },
      }),
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
          selfMute: false,
          selfDeaf: false,
        }),
      }),
    ]);
    await settle();

    unmountHook = (await renderHook(() => useVoiceCues())).unmount;
    core.calls.length = 0;

    // Update 1: self mute arrives
    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
          selfMute: true,
          selfDeaf: false,
        }),
      }),
    ]);
    await settle();

    // Update 2: self deaf arrives shortly after
    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
          selfMute: true,
          selfDeaf: true,
        }),
      }),
    ]);
    await settle();

    // Fast-forward debounce timer (50ms)
    await new Promise((r) => setTimeout(r, 80));

    const cues = core.calls
      .filter((c) => c.method === 'cuePlay')
      .map((c) => c.args[0]);
    expect(cues).toEqual([VoiceCueKind.Deafen]);
  });

  // Fails if a cue group in cuesOff plays its cues.
  it('a group in cuesOff plays nothing', async () => {
    unmountHook = (await renderHook(() => useVoiceCues())).unmount;

    useAppStore.setState({
      prefs: {
        ...useAppStore.getState().prefs,
        cuesOff: ['joinLeave', 'mute', 'deafen', 'othersJoinLeave'],
      },
    });

    const snap = snapshot(NIGHT_OWLS);
    const selfUser = user(SELF_ID);
    onEvents([
      new MobileEvent.Ready({
        snapshot: {
          ...snap,
          selfUser,
          voiceStates: [],
        },
      }),
    ]);
    await settle();

    core.calls.length = 0;

    // Self joins lounge
    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: makeVoiceState({
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
        }),
      }),
    ]);
    await settle();

    await new Promise((r) => setTimeout(r, 80));

    const cues = core.calls.filter((c) => c.method === 'cuePlay');
    expect(cues).toHaveLength(0);
  });
});
