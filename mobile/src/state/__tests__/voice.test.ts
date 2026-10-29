import { ConnectionRecord, MobileEvent } from '@opencord/core-native';
import { VoiceService } from '@opencord/voice-service';
import { PermissionStatus, type PermissionResponse } from 'expo';
import * as Audio from 'expo-audio';
import { Alert, AppState, Linking, type AppStateStatus } from 'react-native';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { startApp } from '@/core/lifecycle';
import { freshStore, settle } from '@/test/store';

import { joinVoice, leaveVoice, setDeafened, setMuted, watchVoiceService } from '../actions/voice';
import { onEvents, useAppStore } from '../store';

jest.mock('expo-audio', () => ({
  getRecordingPermissionsAsync: jest.fn(),
  requestRecordingPermissionsAsync: jest.fn(),
  requestNotificationPermissionsAsync: jest.fn(),
  setAudioModeAsync: jest.fn(async () => {}),
}));

const audio = jest.mocked(Audio);
const service = jest.mocked(VoiceService);

function microphone(granted: boolean, canAskAgain = true): PermissionResponse {
  return { granted, canAskAgain, status: granted ? PermissionStatus.GRANTED : PermissionStatus.DENIED, expires: 'never' };
}

/** Applies what the core sent, as the event loop would. */
async function pump(core: FakeCore): Promise<void> {
  onEvents(await core.nextEvents(core.eventsAttach(), 100));
}

/** Each call in the order it happened, by name. */
function order(core: FakeCore): string[] {
  const names: string[] = [];
  const marks: [number, string][] = [
    ...audio.setAudioModeAsync.mock.invocationCallOrder.map((at): [number, string] => [at, 'audioMode']),
    ...service.start.mock.invocationCallOrder.map((at: number): [number, string] => [at, 'serviceStart']),
    ...service.stop.mock.invocationCallOrder.map((at: number): [number, string] => [at, 'serviceStop']),
  ];
  for (const [, name] of marks.sort(([a], [b]) => a - b)) names.push(name);
  return [...names, ...core.calls.filter((call) => call.method.startsWith('voice')).map((call) => call.method)];
}

describe('voice', () => {
  let core: FakeCore;
  let unwatch: () => void;

  beforeEach(async () => {
    jest.clearAllMocks();
    core = freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();
    audio.getRecordingPermissionsAsync.mockResolvedValue(microphone(true));
    unwatch = watchVoiceService();
  });

  afterEach(() => unwatch());

  it('joining asks for the microphone first and stops if refused', async () => {
    // Expo can say it can't ask again when Android still would (it
    // remembers asking before a revoke): the app asks anyway.
    audio.getRecordingPermissionsAsync.mockResolvedValue(microphone(false, false));
    audio.requestRecordingPermissionsAsync.mockResolvedValue(microphone(false, false));
    const alert = jest.spyOn(Alert, 'alert');

    await expect(joinVoice(NIGHT_OWLS, CHANNELS.lounge)).resolves.toBe(false);

    expect(audio.requestRecordingPermissionsAsync).toHaveBeenCalledTimes(1);
    expect(core.calls.some((call) => call.method === 'voiceJoin')).toBe(false);
    expect(service.start).not.toHaveBeenCalled();
    expect(audio.setAudioModeAsync).not.toHaveBeenCalled();

    // Spec §8.6's sentence, with Open Settings.
    expect(alert).toHaveBeenCalledWith(
      'Opencord needs the microphone for voice.',
      'You can allow it in Settings.',
      expect.any(Array),
    );
    const openSettings = jest.spyOn(Linking, 'openSettings').mockResolvedValueOnce();
    const buttons = alert.mock.calls[0]?.[2] ?? [];
    buttons.find((button) => button.text === 'Open Settings')?.onPress?.();
    expect(openSettings).toHaveBeenCalled();
  });

  it('joining sets the audio session, starts the service, then joins', async () => {
    await expect(joinVoice(NIGHT_OWLS, CHANNELS.lounge)).resolves.toBe(true);
    await pump(core);

    expect(order(core)).toEqual(['audioMode', 'serviceStart', 'voiceJoin']);
    expect(audio.setAudioModeAsync).toHaveBeenCalledWith({
      allowsRecording: true,
      playsInSilentMode: true,
      shouldPlayInBackground: true,
      interruptionMode: 'doNotMix',
    });
    expect(service.start).toHaveBeenLastCalledWith({ title: 'In voice: Lounge · Night Owls', text: '' });
    expect(useAppStore.getState().myVoice).toMatchObject({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge });
  });

  it('leaving stops the service', async () => {
    await joinVoice(NIGHT_OWLS, CHANNELS.lounge);
    await pump(core);

    await leaveVoice();
    await pump(core);

    expect(core.calls.some((call) => call.method === 'voiceLeave')).toBe(true);
    expect(service.stop).toHaveBeenCalled();
    expect(useAppStore.getState().myVoice).toBeNull();
    // The session lets other apps play again.
    expect(audio.setAudioModeAsync).toHaveBeenLastCalledWith(
      expect.objectContaining({ allowsRecording: false, shouldPlayInBackground: false }),
    );
  });

  it('a join that fails stops the service again', async () => {
    await expect(joinVoice(RUST_CLUB, CHANNELS.lounge)).rejects.toThrow();
    expect(service.start).toHaveBeenCalledTimes(1);
    expect(service.stop).toHaveBeenCalledTimes(1);
    expect(useAppStore.getState().myVoice).toBeNull();
  });

  it('moving to another channel keeps one service', async () => {
    await joinVoice(NIGHT_OWLS, CHANNELS.lounge);
    await pump(core);
    await joinVoice(NIGHT_OWLS, CHANNELS.quiet);
    await pump(core);

    expect(service.stop).not.toHaveBeenCalled();
    // Starting a running service only changes its notification.
    expect(service.start).toHaveBeenLastCalledWith({ title: 'In voice: Quiet room · Night Owls', text: '' });
    expect(useAppStore.getState().myVoice).toMatchObject({ channelId: CHANNELS.quiet });
  });

  it('voice ending without a leave here stops the service', async () => {
    await joinVoice(NIGHT_OWLS, CHANNELS.lounge);
    await pump(core);
    service.stop.mockClear();

    // Another device took over, or the server moved the user out.
    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: {
          userId: '1000',
          channelId: undefined,
          thisDevice: true,
          selfMute: false,
          selfDeaf: false,
          serverMute: false,
          serverDeaf: false,
        },
      }),
    ]);

    expect(useAppStore.getState().myVoice).toBeNull();
    expect(service.stop).toHaveBeenCalledTimes(1);
  });

  it('mute and deafen reach the core', async () => {
    await joinVoice(NIGHT_OWLS, CHANNELS.lounge);
    setMuted(true);
    setDeafened(true);
    await pump(core);

    expect(core.calls.filter((call) => call.method === 'voiceSetMute' || call.method === 'voiceSetDeaf')).toEqual([
      { method: 'voiceSetMute', args: [true] },
      { method: 'voiceSetDeaf', args: [true] },
    ]);
    expect(useAppStore.getState().myVoice).toMatchObject({ muted: true, deafened: true });
  });
});

describe('coming back', () => {
  it('coming back to the foreground retries the servers that aren\'t connected', async () => {
    const core = freshStore();
    let onChange: ((state: AppStateStatus) => void) | undefined;
    jest.spyOn(AppState, 'addEventListener').mockImplementation((_type, listener) => {
      onChange = listener as (state: AppStateStatus) => void;
      return { remove: () => {} } as ReturnType<typeof AppState.addEventListener>;
    });
    startApp();
    onEvents([new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() })]);
    core.calls.length = 0;

    onChange?.('active');
    await settle();

    const retried = core.calls.filter((call) => call.method === 'serverRetryNow').map((call) => call.args[0]);
    expect(retried).not.toContain(NIGHT_OWLS);
    expect(retried.length).toBe(Object.keys(useAppStore.getState().servers).length - 1);
  });
});
