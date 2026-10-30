import { act, fireEvent, render, screen, within } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, RUST_CLUB, SELF_ID, snapshot } from '@/core/fake/world';
import { forgetDefaultSounds } from '@/state/actions/soundboard';
import { onEvents, useAppStore } from '@/state/store';
import { channelKey } from '@/state/types';
import { freshStore, settle } from '@/test/store';

import { ParticipantTile } from '@/features/voice/ParticipantTile';

import { favoriteKey } from '../picker';
import { SoundboardSheet } from '../SoundboardSheet';

const AIRHORN = `server:${'a'.repeat(64)}`;
const FERRIS = 'd'.repeat(64);

async function pump(core: FakeCore): Promise<void> {
  onEvents(await core.nextEvents(core.eventsAttach(), 100));
}

async function inVoice(): Promise<FakeCore> {
  const core = freshStore();
  forgetDefaultSounds();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(RUST_CLUB) }),
  ]);
  await settle();
  await core.voiceJoin(NIGHT_OWLS, CHANNELS.lounge);
  await pump(core);
  return core;
}

const toasts = () => useAppStore.getState().toasts.map((toast) => toast.text);
const plays = (core: FakeCore) => core.calls.filter((call) => call.method === 'soundboardPlay').map((call) => call.args);

describe('the soundboard sheet', () => {
  it('plays a sound for the channel, and its ring shows on the player', async () => {
    const core = await inVoice();
    await render(<SoundboardSheet />);

    await act(async () => {
      fireEvent.press(screen.getByLabelText('Play Airhorn'));
    });
    await pump(core);

    expect(plays(core)).toEqual([[NIGHT_OWLS, CHANNELS.lounge, AIRHORN, undefined]]);
    expect(useAppStore.getState().soundRings[channelKey(NIGHT_OWLS, CHANNELS.lounge)]?.[SELF_ID]).toBeDefined();
    expect(useAppStore.getState().lastSound[NIGHT_OWLS]).toBeDefined();
  });

  it("rings the player's tile while the sound plays, then not", async () => {
    await inVoice();
    const member = { userId: '1001', name: 'Kai', muted: false, deafened: false, speaking: false, isSelf: false };
    await render(<ParticipantTile serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} member={member} size={160} />);
    expect(screen.queryByTestId('participant-sound-ring')).toBeNull();

    jest.useFakeTimers({ now: Date.now() });
    try {
      await act(async () => {
        onEvents([
          new MobileEvent.SoundPlayed({
            serverKey: NIGHT_OWLS,
            effect: { channelId: CHANNELS.lounge, userId: '1001', soundRef: 'default:tada', name: 'Ta-da', emoji: '🎉', startedAtMs: 0, durationMs: 1_600 },
          }),
        ]);
      });
      expect(screen.getByTestId('participant-sound-ring')).toBeTruthy();

      await act(async () => {
        jest.advanceTimersByTime(1_600);
      });
      expect(screen.queryByTestId('participant-sound-ring')).toBeNull();
    } finally {
      jest.useRealTimers();
    }
  });

  it("plays another server's sound as an external one", async () => {
    const core = await inVoice();
    await render(<SoundboardSheet />);

    await act(async () => {
      fireEvent.press(screen.getByLabelText('Play Ferris'));
    });

    expect(plays(core)).toEqual([
      [NIGHT_OWLS, CHANNELS.lounge, `external:${FERRIS}`, expect.objectContaining({ serverKey: RUST_CLUB, sound: expect.objectContaining({ sha256: FERRIS }) })],
    ]);
  });

  it('says to wait during the cooldown', async () => {
    await inVoice();
    await render(<SoundboardSheet />);

    await act(async () => {
      fireEvent.press(screen.getByLabelText('Play Airhorn'));
    });
    await act(async () => {
      fireEvent.press(screen.getByLabelText('Play Owl hoot'));
    });

    expect(toasts()).toContain('Wait a moment to play another sound.');
  });

  it('dims every sound while deafened, and a tap says why', async () => {
    const core = await inVoice();
    useAppStore.setState((state) => ({ myVoice: state.myVoice && { ...state.myVoice, deafened: true } }));
    await render(<SoundboardSheet />);

    expect(screen.getByText("You can't play sounds while deafened")).toBeTruthy();
    await act(async () => {
      fireEvent.press(screen.getByLabelText("Airhorn, You can't play sounds while deafened"));
    });
    expect(plays(core)).toEqual([]);
    expect(toasts()).toContain("You can't play sounds while deafened");
  });

  it('a long press previews here or makes a favourite, kept in the settings', async () => {
    const core = await inVoice();
    await render(<SoundboardSheet />);
    const key = favoriteKey(AIRHORN, NIGHT_OWLS);
    const menu = () => within(screen.getByTestId(`sound-menu-${key}`));

    await act(async () => {
      fireEvent.press(menu().getByRole('menuitem', { name: 'Preview' }));
    });
    expect(core.calls.filter((call) => call.method === 'soundboardPreview').map((call) => call.args)).toEqual([[NIGHT_OWLS, AIRHORN]]);

    await act(async () => {
      fireEvent.press(menu().getByRole('menuitem', { name: 'Add to Favorites' }));
    });
    expect(screen.getByText('Favorites')).toBeTruthy();
    expect(core.settingsGet('mobile.soundFavorites')).toBe(JSON.stringify([key]));
  });

  it('searches by name', async () => {
    await inVoice();
    await render(<SoundboardSheet />);

    await act(async () => {
      fireEvent.changeText(screen.getByPlaceholderText('Search sounds'), 'owl');
    });
    expect(screen.getByLabelText('Play Owl hoot')).toBeTruthy();
    expect(screen.queryByLabelText('Play Airhorn')).toBeNull();
  });
});
