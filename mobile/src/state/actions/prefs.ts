// The person's preferences, kept in the core's settings: the theme, their
// music volume, mute and ducking (the core's own defaults), and their
// soundboard favourites.
import { getCore } from '@/core';

import { DEFAULT_MUSIC, useAppStore } from '../store';
import type { MusicListening, Theme } from '../types';

const THEME = 'mobile.theme';
const MUSIC = 'mobile.musicListening';
const SOUND_FAVORITES = 'mobile.soundFavorites';

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

export function loadPrefs(): void {
  const core = getCore();
  const theme = core.settingsGet(THEME);
  const music = readMusic(core.settingsGet(MUSIC));
  const soundFavorites = readFavorites(core.settingsGet(SOUND_FAVORITES));
  useAppStore.setState({
    prefs: { theme: theme === 'light' || theme === 'dark' ? theme : 'system', music, soundFavorites },
  });
  core.musicSetListening(music.volume, music.muted, music.duck);
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
