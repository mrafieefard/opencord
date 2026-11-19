import { act, cleanup, fireEvent, render, screen } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent, NoiseSuppression } from '@opencord/core-native';
import { PermissionStatus, type PermissionResponse } from 'expo';
import * as Audio from 'expo-audio';
import { Alert } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { watchAudioEvents } from '@/state/actions/audio';
import { setAppActive } from '@/state/actions/ui';
import { joinVoice } from '@/state/actions/voice';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { VoiceAudioPage } from '../VoiceAudioPage';

jest.mock('expo-haptics', () => ({
  impactAsync: jest.fn(async () => {}),
  ImpactFeedbackStyle: { Medium: 'medium' },
}));

jest.mock('expo-router', () => ({
  useNavigation: () => ({ setOptions: jest.fn(), addListener: jest.fn(() => () => {}) }),
  // Focus is the page's life here: it comes on mount and goes on unmount.
  useFocusEffect: (effect: () => undefined | (() => void)) =>
    jest.requireActual<typeof import('react')>('react').useEffect(effect, [effect]),
}));

const audio = jest.mocked(Audio);

function microphone(granted: boolean): PermissionResponse {
  return { granted, canAskAgain: true, status: granted ? PermissionStatus.GRANTED : PermissionStatus.DENIED, expires: 'never' };
}

const FELL_BACK = 'High noise suppression is too heavy for this phone. Using Standard.';

/** Applies what the core sent, as the event loop would. */
async function pump(core: FakeCore): Promise<void> {
  onEvents(await core.nextEvents(core.eventsAttach(), 100));
}

/** The arguments of each call to one method, in order. */
function argsOf(core: FakeCore, method: string): unknown[][] {
  return core.calls.filter((call) => call.method === method).map((call) => call.args);
}

async function draw() {
  return await render(
    <GestureHandlerRootView style={{ flex: 1 }}>
      <VoiceAudioPage />
    </GestureHandlerRootView>,
  );
}

const texts = () => useAppStore.getState().toasts.map((entry) => entry.text);

describe('Voice & audio', () => {
  let core: FakeCore;

  beforeEach(async () => {
    jest.clearAllMocks();
    // Granted, unless a test says otherwise (clearAllMocks keeps a test's own answers).
    audio.getRecordingPermissionsAsync.mockResolvedValue(microphone(true));
    audio.requestRecordingPermissionsAsync.mockResolvedValue(microphone(true));
    core = freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();
    // As startApp does: the store's media events reach actions/audio.ts.
    watchAudioEvents();
  });

  afterEach(async () => {
    // Unmounting ends any test still running; let it finish on this core.
    await cleanup();
    await settle();
  });

  // One person, this device: the test runs on Voice & audio, then this
  // device joins Lounge on Night Owls.
  it('joining a call stops the test', async () => {
    await draw();
    await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
    await settle();
    expect(screen.getByRole('button', { name: 'Stop test' })).toBeTruthy();
    expect(argsOf(core, 'micTestSet')).toEqual([[true]]);
    expect(argsOf(core, 'levelMeterSet')).toEqual([[true]]);
    audio.setIsAudioActiveAsync.mockClear();

    await act(async () => {
      await joinVoice(NIGHT_OWLS, CHANNELS.lounge);
      await pump(core);
    });
    await settle();

    // Off before the core joined: the core carries a running test into the call.
    const order = core.calls
      .map((call) => call.method)
      .filter((method) => method === 'micTestSet' || method === 'levelMeterSet' || method === 'voiceJoin');
    expect(order).toEqual(['levelMeterSet', 'micTestSet', 'levelMeterSet', 'micTestSet', 'voiceJoin']);
    expect(argsOf(core, 'micTestSet')).toEqual([[true], [false]]);
    expect(argsOf(core, 'levelMeterSet')).toEqual([[true], [false]]);
    // The test kept the audio session for the call: it was turned on for
    // the call and never off (iOS would cut the call's audio).
    expect(audio.setIsAudioActiveAsync.mock.calls).toEqual([[true]]);
    expect(useAppStore.getState().micTest).toBe(false);

    // In the call the button can't start it, pressed twice.
    const button = screen.getByRole('button', { name: 'Test' });
    expect(button).toBeDisabled();
    expect(screen.getByText('Leave the call to test your microphone.')).toBeTruthy();
    expect(screen.queryByText('Hear what Opencord sends. Use headphones, or you may hear an echo.')).toBeNull();
    await fireEvent.press(button);
    await fireEvent.press(button);
    await settle();
    expect(argsOf(core, 'micTestSet')).toEqual([[true], [false]]);
  });

  it('leaving the page turns the meter off', async () => {
    for (const round of [1, 2]) {
      const view = await draw();
      expect(screen.getByText('Hear what Opencord sends. Use headphones, or you may hear an echo.')).toBeTruthy();
      await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
      await settle();
      // The level the core sends (events.rs: InputLevel { dbfs }) fills the bar: −30 dBFS is half of −60…0.
      await act(async () => onEvents([new MobileEvent.InputLevel({ dbfs: -30 })]));
      expect(screen.getByLabelText('Input level').props.accessibilityValue).toMatchObject({ now: 50 });
      audio.setIsAudioActiveAsync.mockClear();

      await view.unmount();
      await settle();

      expect(argsOf(core, 'levelMeterSet')).toEqual(round === 1 ? [[true], [false]] : [[true], [false], [true], [false]]);
      expect(argsOf(core, 'micTestSet')).toEqual(round === 1 ? [[true], [false]] : [[true], [false], [true], [false]]);
      expect(useAppStore.getState().micTest).toBe(false);
      // Not in a call: the session the test took goes back.
      expect(audio.setIsAudioActiveAsync.mock.calls).toEqual([[false]]);
    }
  });

  it('going to the background stops the test', async () => {
    await draw();
    for (const round of [1, 2]) {
      await act(async () => setAppActive(true));
      await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
      await settle();
      await act(async () => setAppActive(false));
      await settle();
      expect(screen.getByRole('button', { name: 'Test' })).toBeTruthy();
      expect(argsOf(core, 'micTestSet')).toEqual(round === 1 ? [[true], [false]] : [[true], [false], [true], [false]]);
    }
  });

  it('Test and Stop test work twice in a row', async () => {
    await draw();
    for (let round = 0; round < 2; round += 1) {
      await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
      await settle();
      expect(screen.getByLabelText('Input level')).toBeTruthy();
      await fireEvent.press(screen.getByRole('button', { name: 'Stop test' }));
      await settle();
      expect(screen.queryByLabelText('Input level')).toBeNull();
    }
    expect(argsOf(core, 'micTestSet')).toEqual([[true], [false], [true], [false]]);
    expect(argsOf(core, 'levelMeterSet')).toEqual([[true], [false], [true], [false]]);
    // The audio session, taken and given back each time (not in a call).
    expect(audio.setIsAudioActiveAsync.mock.calls).toEqual([[true], [false], [true], [false]]);
  });

  it('a refused microphone starts nothing', async () => {
    audio.getRecordingPermissionsAsync.mockResolvedValue(microphone(false));
    audio.requestRecordingPermissionsAsync.mockResolvedValue(microphone(false));
    const alert = jest.spyOn(Alert, 'alert');
    await draw();
    await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
    await settle();
    await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
    await settle();
    expect(argsOf(core, 'micTestSet')).toEqual([]);
    expect(audio.setIsAudioActiveAsync).not.toHaveBeenCalled();
    expect(alert.mock.calls.map((call) => call[0])).toEqual([
      'Opencord needs the microphone for voice.',
      'Opencord needs the microphone for voice.',
    ]);
    expect(useAppStore.getState().micTest).toBe(false);
    alert.mockRestore();
  });

  // The system's permission prompt takes the app out of the foreground
  // (which ends a running test); the test asked for still starts once granted.
  it('the permission prompt does not end the test it is asked for', async () => {
    let grant = () => {};
    audio.getRecordingPermissionsAsync.mockResolvedValue(microphone(false));
    audio.requestRecordingPermissionsAsync.mockImplementationOnce(
      () =>
        new Promise<PermissionResponse>((resolve) => {
          grant = () => resolve(microphone(true));
        }),
    );
    await draw();
    await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
    await settle();
    await act(async () => setAppActive(false));
    await act(async () => setAppActive(true));
    await act(async () => grant());
    await settle();
    expect(argsOf(core, 'micTestSet')).toEqual([[true]]);
    expect(screen.getByRole('button', { name: 'Stop test' })).toBeTruthy();
  });

  // Task 2's finding: micTestSet runs on a blocking thread, so an "off" sent
  // while the "on" is still opening the microphone must wait for it.
  it('stopping while the test is still starting waits for the start', async () => {
    await draw();
    let release = () => {};
    const micTestSet = jest.spyOn(core, 'micTestSet');
    micTestSet.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
    await settle();
    await fireEvent.press(screen.getByRole('button', { name: 'Stop test' }));
    await settle();
    expect(micTestSet.mock.calls).toEqual([[true]]);
    expect(screen.getByRole('button', { name: 'Test' })).toBeTruthy();

    await act(async () => release());
    await settle();
    expect(micTestSet.mock.calls).toEqual([[true], [false]]);
    expect(argsOf(core, 'levelMeterSet')).toEqual([[true], [false]]);
    expect(useAppStore.getState().micTest).toBe(false);
  });

  it("a microphone that can't open says why and ends the test", async () => {
    await draw();
    await fireEvent.press(screen.getByRole('button', { name: 'Test' }));
    await settle();
    // What the facade sends when the test's own engine fails to start
    // (media.rs DeviceFailed { output: false } → AudioProblem).
    await act(async () =>
      onEvents([new MobileEvent.AudioProblem({ output: false, fellBack: false, message: 'no input device available' })]),
    );
    await settle();
    expect(texts()).toContain('no input device available');
    expect(argsOf(core, 'micTestSet')).toEqual([[true], [false]]);
    expect(screen.getByRole('button', { name: 'Test' })).toBeTruthy();
  });

  it('a fallback moves the choice to Standard', async () => {
    await draw();
    for (const round of [1, 2]) {
      await fireEvent.press(screen.getByRole('radio', { name: 'High' }));
      expect(screen.getByRole('radio', { name: 'High' }).props.accessibilityState).toMatchObject({ checked: true });
      expect(argsOf(core, 'audioSettingsApply').at(-1)).toEqual([
        { noiseSuppression: NoiseSuppression.High, inputVolume: 1, outputVolume: 1 },
      ]);

      // What the core sends when High can't keep up on this phone.
      await act(async () => onEvents([new MobileEvent.NoiseSuppressionFellBack()]));

      expect(screen.getByRole('radio', { name: 'Standard' }).props.accessibilityState).toMatchObject({ checked: true });
      expect(screen.getByRole('radio', { name: 'High' }).props.accessibilityState).toMatchObject({ checked: false });
      expect(argsOf(core, 'audioSettingsApply').at(-1)).toEqual([
        { noiseSuppression: NoiseSuppression.Standard, inputVolume: 1, outputVolume: 1 },
      ]);
      expect(JSON.parse(core.settingsGet('mobile.audio') ?? '{}')).toEqual({
        noiseSuppression: 'standard',
        inputVolume: 1,
        outputVolume: 1,
      });
      expect(texts().filter((text) => text === FELL_BACK)).toHaveLength(round);
    }
  });

  it('a fallback after another choice keeps that choice', async () => {
    await draw();
    await fireEvent.press(screen.getByRole('radio', { name: 'Off' }));
    await act(async () => onEvents([new MobileEvent.NoiseSuppressionFellBack()]));
    expect(screen.getByRole('radio', { name: 'Off' }).props.accessibilityState).toMatchObject({ checked: true });
    expect(texts()).not.toContain(FELL_BACK);
  });

  it('each noise suppression row chooses its mode', async () => {
    await draw();
    const choices: [string, NoiseSuppression, string][] = [
      ['Off', NoiseSuppression.Off, 'off'],
      ['High', NoiseSuppression.High, 'high'],
      ['Standard', NoiseSuppression.Standard, 'standard'],
    ];
    expect(screen.getByText('Sends the microphone as it is')).toBeTruthy();
    expect(screen.getByText('Removes steady noise; very light')).toBeTruthy();
    expect(screen.getByText('Also keyboards and dogs; heavier')).toBeTruthy();
    for (const [label, mode, stored] of choices) {
      const before = argsOf(core, 'audioSettingsApply').length;
      await fireEvent.press(screen.getByRole('radio', { name: label }));
      await fireEvent.press(screen.getByRole('radio', { name: label }));
      expect(argsOf(core, 'audioSettingsApply').slice(before)).toEqual([
        [{ noiseSuppression: mode, inputVolume: 1, outputVolume: 1 }],
        [{ noiseSuppression: mode, inputVolume: 1, outputVolume: 1 }],
      ]);
      expect(screen.getByRole('radio', { name: label }).props.accessibilityState).toMatchObject({ checked: true });
      expect(JSON.parse(core.settingsGet('mobile.audio') ?? '{}').noiseSuppression).toBe(stored);
    }
  });

  it('volumes apply when the slider is let go', async () => {
    await draw();
    expect(screen.getAllByText('100 %')).toHaveLength(2);
    const applied = () => argsOf(core, 'audioSettingsApply').length;
    const before = applied();

    // SwiftUI's slider: moving calls onValueChange, letting go onEditingChanged(false).
    const input = screen.getByLabelText('Input volume');
    await fireEvent(input, 'valueChange', 150);
    await fireEvent(input, 'valueChange', 160);
    expect(screen.getByText('160 %')).toBeTruthy();
    expect(applied()).toBe(before);
    await fireEvent(input, 'editingChanged', false);
    expect(applied()).toBe(before + 1);
    expect(argsOf(core, 'audioSettingsApply').at(-1)).toEqual([
      { noiseSuppression: NoiseSuppression.Standard, inputVolume: 1.6, outputVolume: 1 },
    ]);

    const output = screen.getByLabelText('Output volume');
    await fireEvent(output, 'valueChange', 40);
    expect(applied()).toBe(before + 1);
    await fireEvent(output, 'editingChanged', false);
    // A second drag of the same slider applies again.
    await fireEvent(output, 'valueChange', 30);
    await fireEvent(output, 'editingChanged', false);
    expect(applied()).toBe(before + 3);
    expect(argsOf(core, 'audioSettingsApply').at(-1)).toEqual([
      { noiseSuppression: NoiseSuppression.Standard, inputVolume: 1.6, outputVolume: 0.3 },
    ]);
    expect(useAppStore.getState().prefs.audio).toEqual({
      noiseSuppression: NoiseSuppression.Standard,
      inputVolume: 1.6,
      outputVolume: 0.3,
    });
    expect(screen.getByText('160 %')).toBeTruthy();
    expect(screen.getByText('30 %')).toBeTruthy();
  });
});
