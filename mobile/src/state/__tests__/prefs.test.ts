import { NoiseSuppression } from '@opencord/core-native';

import { freshStore } from '@/test/store';

import { loadPrefs, setAudio, setMusicListening } from '../actions/prefs';
import { useAppStore } from '../store';

describe('preferences', () => {
  it("music listening starts at the core's defaults and applies on change", () => {
    const core = freshStore();
    loadPrefs();
    expect(useAppStore.getState().prefs.music).toEqual({ volume: 0.5, muted: false, duck: false });
    expect(core.calls.filter((call) => call.method === 'musicSetListening').at(-1)?.args).toEqual([0.5, false, false]);
    setMusicListening({ volume: 1.2, duck: true });
    expect(core.calls.filter((call) => call.method === 'musicSetListening').at(-1)?.args).toEqual([1.2, false, true]);
    expect(JSON.parse(core.settingsGet('mobile.musicListening') ?? '{}')).toEqual({ volume: 1.2, muted: false, duck: true });
    // Kept: a new start reads it back.
    loadPrefs();
    expect(useAppStore.getState().prefs.music).toEqual({ volume: 1.2, muted: false, duck: true });
  });

  it('a volume out of range is clamped to 0–200 %', () => {
    freshStore();
    setMusicListening({ volume: 3 });
    expect(useAppStore.getState().prefs.music.volume).toBe(2);
  });

  // Fails if the phone's untouched audio isn't the core's own default
  // (Standard, 100 %, 100 %), if boot doesn't hand it to the core, or if a
  // change isn't kept for the next start.
  it("voice audio starts at the core's defaults, applies at boot and is kept", () => {
    const core = freshStore();
    loadPrefs();
    const defaults = { noiseSuppression: NoiseSuppression.Standard, inputVolume: 1, outputVolume: 1 };
    expect(useAppStore.getState().prefs.audio).toEqual(defaults);
    expect(core.calls.filter((call) => call.method === 'audioSettingsApply').at(-1)?.args).toEqual([defaults]);

    setAudio({ noiseSuppression: NoiseSuppression.High, inputVolume: 2.5 });
    const changed = { noiseSuppression: NoiseSuppression.High, inputVolume: 2, outputVolume: 1 };
    expect(core.calls.filter((call) => call.method === 'audioSettingsApply').at(-1)?.args).toEqual([changed]);
    // Kept by name, not by the bindings' enum number.
    expect(JSON.parse(core.settingsGet('mobile.audio') ?? '{}')).toEqual({ noiseSuppression: 'high', inputVolume: 2, outputVolume: 1 });

    loadPrefs();
    expect(useAppStore.getState().prefs.audio).toEqual(changed);
    expect(core.calls.filter((call) => call.method === 'audioSettingsApply').at(-1)?.args).toEqual([changed]);
  });

  it('stored voice audio that makes no sense falls back value by value', () => {
    const core = freshStore();
    core.settingsSet('mobile.audio', JSON.stringify({ noiseSuppression: 'loud', inputVolume: 'x', outputVolume: -1 }));
    loadPrefs();
    expect(useAppStore.getState().prefs.audio).toEqual({ noiseSuppression: NoiseSuppression.Standard, inputVolume: 1, outputVolume: 0 });
    setAudio({ inputVolume: Number.NaN });
    expect(useAppStore.getState().prefs.audio.inputVolume).toBe(1);
  });
});
