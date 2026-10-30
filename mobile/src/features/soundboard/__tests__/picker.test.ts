import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, RUST_CLUB, SELF_ID, defaultSounds, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { favoriteKey, soundboardPicker } from '../picker';

async function inVoice() {
  freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(RUST_CLUB) }),
  ]);
  await settle();
  useAppStore.setState({
    myVoice: { serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, link: null, muted: false, deafened: false },
  });
}

const titles = () => soundboardPicker(useAppStore.getState(), defaultSounds, '')?.sections.map((section) => section.title);

describe('the soundboard picker', () => {
  it('is nothing outside voice', async () => {
    await inVoice();
    useAppStore.setState({ myVoice: null });
    expect(soundboardPicker(useAppStore.getState(), defaultSounds, '')).toBeNull();
  });

  it("has this server's sounds, each other server's, then the defaults", async () => {
    await inVoice();
    const picker = soundboardPicker(useAppStore.getState(), defaultSounds, '');
    expect(picker?.sections.map((section) => [section.title, section.sounds.map((sound) => sound.name)])).toEqual([
      ['This server', ['Airhorn', 'Owl hoot', 'Rimshot']],
      ['Rust Club', ['Ferris']],
      ['Default', ['Chime', 'Ta-da', 'Air horn', 'Applause']],
    ]);
    expect(picker?.blocked).toBeNull();
    expect(picker?.cooldownMs).toBe(3_000);
  });

  it("plays another server's sound here as external, previewing it where it lives", async () => {
    await inVoice();
    const ferris = soundboardPicker(useAppStore.getState(), defaultSounds, '')?.sections[1]?.sounds[0];
    expect(ferris).toMatchObject({
      ref: `external:${'d'.repeat(64)}`,
      previewServer: RUST_CLUB,
      previewRef: `server:${'d'.repeat(64)}`,
      from: { serverKey: RUST_CLUB, sound: { id: '800101' } },
    });
  });

  it('puts favourites first and searches by name', async () => {
    await inVoice();
    useAppStore.setState((state) => ({
      prefs: { ...state.prefs, soundFavorites: [favoriteKey('default:tada'), favoriteKey(`server:${'b'.repeat(64)}`, NIGHT_OWLS)] },
    }));
    expect(titles()).toEqual(['Favorites', 'This server', 'Rust Club', 'Default']);
    expect(soundboardPicker(useAppStore.getState(), defaultSounds, '')?.sections[0]?.sounds.map((sound) => sound.name)).toEqual([
      'Owl hoot',
      'Ta-da',
    ]);

    const found = soundboardPicker(useAppStore.getState(), defaultSounds, ' HORN ');
    expect(found?.sections.map((section) => [section.title, section.sounds.map((sound) => sound.name)])).toEqual([
      ['This server', ['Airhorn']],
      ['Default', ['Air horn']],
    ]);
    expect(soundboardPicker(useAppStore.getState(), defaultSounds, 'nothing like it')?.empty).toBe(true);
  });

  it('says why a sound cannot be played', async () => {
    await inVoice();
    const reasons = () =>
      soundboardPicker(useAppStore.getState(), defaultSounds, '')?.sections.map((section) => [
        section.title,
        section.sounds[0]?.reason,
      ]);

    useAppStore.setState((state) => ({ myVoice: state.myVoice && { ...state.myVoice, deafened: true } }));
    expect(soundboardPicker(useAppStore.getState(), defaultSounds, '')?.blocked).toBe("You can't play sounds while deafened");
    useAppStore.setState((state) => ({ myVoice: state.myVoice && { ...state.myVoice, deafened: false } }));

    onEvents([
      new MobileEvent.SoundboardSettingsChanged({
        serverKey: NIGHT_OWLS,
        settings: { enabled: true, allowDefaultSounds: false, allowExternalSounds: false, cooldownMs: 0 },
      }),
    ]);
    expect(reasons()).toEqual([
      ['This server', null],
      ['Rust Club', "This server doesn't allow sounds from other servers"],
      ['Default', 'This server turned off default sounds'],
    ]);

    onEvents([
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: { userId: SELF_ID, channelId: CHANNELS.lounge, thisDevice: false, selfMute: false, selfDeaf: false, serverMute: true, serverDeaf: false },
      }),
    ]);
    expect(soundboardPicker(useAppStore.getState(), defaultSounds, '')?.blocked).toBe("You can't play sounds while server muted");
  });

  it('follows sounds added and removed', async () => {
    await inVoice();
    onEvents([
      new MobileEvent.SoundboardSoundDeleted({ serverKey: NIGHT_OWLS, soundId: '800001' }),
      new MobileEvent.SoundboardSoundUpserted({
        serverKey: NIGHT_OWLS,
        sound: {
          id: '800004',
          name: 'Bell',
          emoji: '🔔',
          volume: 100,
          sha256: 'e'.repeat(64),
          sizeBytes: 1,
          durationMs: 500,
          uploaderId: undefined,
          createdAtMs: 0,
        },
      }),
    ]);
    expect(soundboardPicker(useAppStore.getState(), defaultSounds, '')?.sections[0]?.sounds.map((sound) => sound.name)).toEqual([
      'Bell',
      'Owl hoot',
      'Rimshot',
    ]);
  });
});
