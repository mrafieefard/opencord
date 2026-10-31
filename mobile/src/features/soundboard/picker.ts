// What the soundboard shows (Phase 2 plan §11.7, as the desktop's
// soundboard_picker.dart): Favorites, This server, each other server
// (their sounds play here as external ones) and Default, filtered by the
// search; and why a sound can't be played here, if so.
import type { DefaultSoundRecord, SoundboardSoundRecord } from '@opencord/core-native';

import type { AppState } from '@/state/types';

export interface PickerSound {
  /** What this server is asked to play. */
  ref: string;
  name: string;
  emoji?: string;
  /** Its key among the favourites. */
  favorite: string;
  /** Another server's sound, and that server. */
  from?: { serverKey: string; sound: SoundboardSoundRecord };
  /** A preview fetches it from this server, by this ref. */
  previewServer: string;
  previewRef: string;
  /** Why it can't be played here; dimmed then. */
  reason: string | null;
}

export interface PickerSection {
  title: string;
  sounds: PickerSound[];
}

export interface Picker {
  serverKey: string;
  channelId: string;
  /** Why nothing can be played here now, if so. */
  blocked: string | null;
  sections: PickerSection[];
  /** No sound matches (or none exists). */
  empty: boolean;
  cooldownMs: number;
}

/** A default sound's ref, or a server's sound by that server and its ref. */
export function favoriteKey(soundRef: string, serverKey?: string): string {
  return serverKey === undefined ? soundRef : `${serverKey} ${soundRef}`;
}

const serverRef = (sound: SoundboardSoundRecord) => `server:${sound.sha256}`;
const externalRef = (sound: SoundboardSoundRecord) => `external:${sound.sha256}`;

/** The soundboard for this device's voice channel; null outside voice. */
export function soundboardPicker(
  state: Pick<AppState, 'myVoice' | 'servers' | 'order' | 'prefs'>,
  defaults: readonly DefaultSoundRecord[],
  query: string,
): Picker | null {
  const voice = state.myVoice;
  if (!voice) return null;
  const { serverKey, channelId } = voice;
  const snapshot = state.servers[serverKey]?.snapshot ?? null;
  const settings = snapshot?.soundboard;
  const access = snapshot?.access.find((entry) => entry.channelId === channelId);
  const selfId = snapshot?.selfUser.id;
  const self = selfId === undefined ? undefined : state.servers[serverKey]?.voice[selfId];

  const blocked =
    settings?.enabled === false
      ? 'The soundboard is turned off on this server'
      : voice.deafened || self?.serverDeaf
        ? "You can't play sounds while deafened"
        : self?.serverMute
          ? "You can't play sounds while server muted"
          : access && !access.useSoundboard
            ? 'You need Use Soundboard here'
            : null;
  const externalBlocked =
    settings?.allowExternalSounds === false
      ? "This server doesn't allow sounds from other servers"
      : access && !access.useExternalSounds
        ? 'You need Use External Sounds here'
        : null;
  const defaultBlocked = settings?.allowDefaultSounds === false ? 'This server turned off default sounds' : null;

  const needle = query.trim().toLowerCase();
  const matches = (name: string) => needle === '' || name.toLowerCase().includes(needle);
  const byName = (a: PickerSound, b: PickerSound) => a.name.localeCompare(b.name);

  const here: PickerSound[] = (snapshot?.soundboardSounds ?? [])
    .filter((sound) => matches(sound.name))
    .map((sound) => ({
      ref: serverRef(sound),
      name: sound.name,
      emoji: sound.emoji,
      favorite: favoriteKey(serverRef(sound), serverKey),
      previewServer: serverKey,
      previewRef: serverRef(sound),
      reason: blocked,
    }))
    .sort(byName);

  const elsewhere: PickerSection[] = state.order
    .filter((key) => key !== serverKey)
    .map((key) => {
      const other = state.servers[key];
      const sounds: PickerSound[] = (other?.snapshot?.soundboardSounds ?? [])
        .filter((sound) => matches(sound.name))
        .map((sound) => ({
          ref: externalRef(sound),
          name: sound.name,
          emoji: sound.emoji,
          favorite: favoriteKey(serverRef(sound), key),
          from: { serverKey: key, sound },
          // Another server's sound previews from where it lives.
          previewServer: key,
          previewRef: serverRef(sound),
          reason: blocked ?? externalBlocked,
        }))
        .sort(byName);
      return { title: other?.snapshot?.name ?? other?.entry.name ?? key, sounds };
    })
    .filter((section) => section.sounds.length > 0);

  const defaultSounds: PickerSound[] = defaults
    .filter((sound) => matches(sound.name))
    .map((sound) => ({
      ref: `default:${sound.id}`,
      name: sound.name,
      emoji: sound.emoji,
      favorite: favoriteKey(`default:${sound.id}`),
      previewServer: serverKey,
      previewRef: `default:${sound.id}`,
      reason: blocked ?? defaultBlocked,
    }));

  const all = [...here, ...elsewhere.flatMap((section) => section.sounds), ...defaultSounds];
  const favorites = new Set(state.prefs.soundFavorites);
  const favored = all.filter((sound) => favorites.has(sound.favorite));

  const sections: PickerSection[] = [
    { title: 'Favorites', sounds: favored },
    { title: 'This server', sounds: here },
    ...elsewhere,
    { title: 'Default', sounds: defaultSounds },
  ].filter((section) => section.sounds.length > 0);

  return {
    serverKey,
    channelId,
    blocked,
    sections,
    empty: all.length === 0,
    cooldownMs: settings?.cooldownMs ?? 0,
  };
}
