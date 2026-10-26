import { freshStore } from '@/test/store';

import { loadPrefs, setMusicListening } from '../actions/prefs';
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
});
