// The person's preferences, kept in the core's settings: the theme, their
// music volume, mute and ducking (the core's own defaults), their
// soundboard favourites, and noise suppression and the voice volumes.
import { NoiseSuppression, type PhoneAudioSettings } from '@opencord/core-native';

import { getCore } from '@/core';

import { DEFAULT_AUDIO, DEFAULT_CUES_OFF, DEFAULT_MUSIC, DEFAULT_SOUNDBOARD, useAppStore } from '../store';
import type { CueGroup, MusicListening, SoundboardPrefs, Theme, UserStatus } from '../types';

const THEME = 'mobile.theme';
const MUSIC = 'mobile.musicListening';
const SOUND_FAVORITES = 'mobile.soundFavorites';
const AUDIO = 'mobile.audio';
export const STATUS = 'mobile.status';
const SOUNDBOARD = 'mobile.soundboard';
const CUES_OFF = 'mobile.cuesOff';

export function readStatus(text: string | undefined): UserStatus {
  if (text === 'idle' || text === 'dnd') return text;
  return 'online';
}

/** Kept by name: the generated enum's numbers are the bindings', not ours. */
const SUPPRESSION_NAMES: [NoiseSuppression, string][] = [
  [NoiseSuppression.Off, 'off'],
  [NoiseSuppression.Standard, 'standard'],
  [NoiseSuppression.High, 'high'],
];

/** 0–2 (200 %); anything that isn't a number is 1, as the facade does. */
function voiceVolume(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 1;
  return Math.min(Math.max(value, 0), 2);
}

function readAudio(text: string | undefined): PhoneAudioSettings {
  if (text === undefined) return { ...DEFAULT_AUDIO };
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === 'object') {
      const value = parsed as Partial<Record<keyof PhoneAudioSettings, unknown>>;
      const suppression = SUPPRESSION_NAMES.find(([, name]) => name === value.noiseSuppression)?.[0];
      return {
        noiseSuppression: suppression ?? DEFAULT_AUDIO.noiseSuppression,
        inputVolume: voiceVolume(value.inputVolume),
        outputVolume: voiceVolume(value.outputVolume),
      };
    }
  } catch {
    console.warn('audio settings could not be parsed; using the defaults');
  }
  return { ...DEFAULT_AUDIO };
}

function writeAudio(audio: PhoneAudioSettings): string {
  const name = SUPPRESSION_NAMES.find(([mode]) => mode === audio.noiseSuppression)?.[1] ?? 'standard';
  return JSON.stringify({ noiseSuppression: name, inputVolume: audio.inputVolume, outputVolume: audio.outputVolume });
}

function readFavorites(text: string | undefined): string[] {
  if (text === undefined) return [];
  try {
    const parsed: unknown = JSON.parse(text);
    if (Array.isArray(parsed)) return parsed.filter((item): item is string => typeof item === 'string');
  } catch {
    console.warn('soundboard favourites could not be parsed; starting without');
  }
  return [];
}

function readMusic(text: string | undefined): MusicListening {
  if (text === undefined) return { ...DEFAULT_MUSIC };
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === 'object') {
      const value = parsed as Partial<Record<keyof MusicListening, unknown>>;
      return {
        volume: typeof value.volume === 'number' ? Math.min(Math.max(value.volume, 0), 2) : DEFAULT_MUSIC.volume,
        muted: typeof value.muted === 'boolean' ? value.muted : DEFAULT_MUSIC.muted,
        duck: typeof value.duck === 'boolean' ? value.duck : DEFAULT_MUSIC.duck,
      };
    }
  } catch {
    console.warn('music settings could not be parsed; using the defaults');
  }
  return { ...DEFAULT_MUSIC };
}

function readSoundboard(text: string | undefined): SoundboardPrefs {
  if (text === undefined) return { ...DEFAULT_SOUNDBOARD };
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === 'object') {
      const value = parsed as Partial<Record<keyof SoundboardPrefs, unknown>>;
      return {
        volume: typeof value.volume === 'number' && Number.isFinite(value.volume) ? Math.min(Math.max(value.volume, 0), 100) : DEFAULT_SOUNDBOARD.volume,
        muted: typeof value.muted === 'boolean' ? value.muted : DEFAULT_SOUNDBOARD.muted,
      };
    }
  } catch {
    console.warn('soundboard settings could not be parsed; using the defaults');
  }
  return { ...DEFAULT_SOUNDBOARD };
}

const CUE_GROUP_NAMES: CueGroup[] = ['joinLeave', 'othersJoinLeave', 'mute', 'deafen', 'stream'];

function readCuesOff(text: string | undefined): CueGroup[] {
  if (text === undefined) return [...DEFAULT_CUES_OFF];
  try {
    const parsed: unknown = JSON.parse(text);
    if (Array.isArray(parsed)) {
      return parsed.filter((item): item is CueGroup => CUE_GROUP_NAMES.includes(item as CueGroup));
    }
  } catch {
    console.warn('cues off settings could not be parsed; starting with all cues on');
  }
  return [...DEFAULT_CUES_OFF];
}

export function loadPrefs(): void {
  const core = getCore();
  const theme = core.settingsGet(THEME);
  const music = readMusic(core.settingsGet(MUSIC));
  const soundFavorites = readFavorites(core.settingsGet(SOUND_FAVORITES));
  const audio = readAudio(core.settingsGet(AUDIO));
  const status = readStatus(core.settingsGet(STATUS));
  const soundboard = readSoundboard(core.settingsGet(SOUNDBOARD));
  const cuesOff = readCuesOff(core.settingsGet(CUES_OFF));
  useAppStore.setState({
    prefs: {
      theme: theme === 'light' || theme === 'dark' ? theme : 'system',
      music,
      soundFavorites,
      audio,
      status,
      soundboard,
      cuesOff,
    },
  });
  core.musicSetListening(music.volume, music.muted, music.duck);
  core.audioSettingsApply(audio);
  core.soundboardSetListening(soundboard.volume / 100, soundboard.muted);
}

export function setSoundboard(change: Partial<SoundboardPrefs>): void {
  const current = useAppStore.getState().prefs.soundboard;
  const soundboard: SoundboardPrefs = {
    volume: typeof change.volume === 'number' && Number.isFinite(change.volume) ? Math.min(Math.max(change.volume, 0), 100) : current.volume,
    muted: typeof change.muted === 'boolean' ? change.muted : current.muted,
  };
  const core = getCore();
  core.soundboardSetListening(soundboard.volume / 100, soundboard.muted);
  core.settingsSet(SOUNDBOARD, JSON.stringify(soundboard));
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, soundboard } }));
}

export function setCueGroup(group: CueGroup, on: boolean): void {
  const current = useAppStore.getState().prefs.cuesOff;
  const cuesOff = on ? current.filter((g) => g !== group) : Array.from(new Set([...current, group]));
  getCore().settingsSet(CUES_OFF, JSON.stringify(cuesOff));
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, cuesOff } }));
}

/** Noise suppression or a volume changed: the core uses it at once, in a call or the microphone test. */
export function setAudio(change: Partial<PhoneAudioSettings>): void {
  const merged = { ...useAppStore.getState().prefs.audio, ...change };
  const audio: PhoneAudioSettings = {
    noiseSuppression: merged.noiseSuppression,
    inputVolume: voiceVolume(merged.inputVolume),
    outputVolume: voiceVolume(merged.outputVolume),
  };
  const core = getCore();
  core.audioSettingsApply(audio);
  core.settingsSet(AUDIO, writeAudio(audio));
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, audio } }));
}

export function setTheme(theme: Theme): void {
  getCore().settingsSet(THEME, theme);
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, theme } }));
}

export function setMusicListening(change: Partial<MusicListening>): void {
  const music = { ...useAppStore.getState().prefs.music, ...change };
  music.volume = Math.min(Math.max(music.volume, 0), 2);
  const core = getCore();
  core.musicSetListening(music.volume, music.muted, music.duck);
  core.settingsSet(MUSIC, JSON.stringify(music));
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, music } }));
}

/** Adds a soundboard favourite, or takes it away. */
export function toggleSoundFavorite(key: string): void {
  const current = useAppStore.getState().prefs.soundFavorites;
  const soundFavorites = current.includes(key) ? current.filter((item) => item !== key) : [...current, key];
  getCore().settingsSet(SOUND_FAVORITES, JSON.stringify(soundFavorites));
  useAppStore.setState((state) => ({ prefs: { ...state.prefs, soundFavorites } }));
}
