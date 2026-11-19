import { fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import {
  ChannelKind,
  ConnectionRecord,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import { useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { VoicePane } from '../VoicePane';
import { VoiceChannelRow } from '@/features/server/VoiceChannelRow';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn() },
}));

jest.mock('expo-audio', () => ({
  getRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestNotificationPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  setAudioModeAsync: jest.fn(async () => {}),
  useAudioPlayer: jest.fn(() => ({
    play: jest.fn(),
    pause: jest.fn(),
    seekTo: jest.fn(),
  })),
  useAudioPlayerStatus: jest.fn(() => ({
    playing: false,
    currentTime: 0,
    duration: 60,
  })),
  useAudioRecorder: jest.fn(() => ({
    prepareToRecordAsync: jest.fn(),
    recordAsync: jest.fn(),
    stopRecording: jest.fn(),
  })),
}));

function renderWithGesture(ui: React.ReactElement) {
  return render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

function initServer(snap = snapshot(NIGHT_OWLS)) {
  const voiceMap = snap.voiceStates
    ? Object.fromEntries(snap.voiceStates.map((v) => [v.userId, v]))
    : {};
  useAppStore.setState((s) => ({
    servers: {
      ...s.servers,
      [NIGHT_OWLS]: {
        key: NIGHT_OWLS,
        entry: { key: NIGHT_OWLS, name: 'Night Owls', host: '127.0.0.1', port: 9000, userId: SELF_ID },
        connection: new ConnectionRecord.Connected(),
        snapshot: snap,
        seeding: false,
        readyCount: 1,
        held: [],
        activity: {},
        typing: {},
        voice: voiceMap,
        nowPlaying: {},
        musicSettings: null,
      },
    },
    order: [NIGHT_OWLS],
  }));
}

describe('Text chat in voice channels (Task 10)', () => {
  beforeEach(async () => {
    freshStore();
    await settle();
    jest.clearAllMocks();
  });

  // Fails if VoicePane does not render the Chat button when textInVoice is true,
  // or pressing it does not open the ChatPane.
  it('a voice channel with text opens its chat', async () => {
    const snap = snapshot(NIGHT_OWLS);
    const lounge = snap.channels.find((c) => c.id === CHANNELS.lounge);
    if (!lounge) throw new Error('missing lounge');
    lounge.textInVoice = true;
    initServer(snap);

    const { getByTestId } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const chatBtn = getByTestId('voice-chat-button');
    expect(chatBtn).toBeTruthy();

    await fireEvent.press(chatBtn);
    expect(getByTestId('chat-messages-container')).toBeTruthy();
  });

  // Fails if VoiceChannelRow does not render the unread pill or mentions badge when unread/mentions > 0.
  it('its unread count shows on the row', async () => {
    const channel = {
      id: 'voice-1',
      kind: ChannelKind.Voice,
      name: 'Lounge',
      position: 0,
      userLimit: 0,
      textInVoice: true,
    };

    const { getByTestId, queryByTestId, rerender } = await render(
      <VoiceChannelRow
        channel={channel}
        count={2}
        limit={0}
        unread={3}
        mentions={1}
        onSelect={jest.fn()}
      />,
    );

    expect(getByTestId('channel-unread-pill')).toBeTruthy();
    expect(getByTestId('channel-mentions-badge')).toBeTruthy();

    await rerender(
      <VoiceChannelRow
        channel={channel}
        count={2}
        limit={0}
        unread={0}
        mentions={0}
        onSelect={jest.fn()}
      />,
    );

    expect(queryByTestId('channel-unread-pill')).toBeNull();
    expect(queryByTestId('channel-mentions-badge')).toBeNull();
  });

  // Fails if VoicePane renders the Chat button when textInVoice is false.
  it('without textInVoice there\'s no button', async () => {
    const snap = snapshot(NIGHT_OWLS);
    const lounge = snap.channels.find((c) => c.id === CHANNELS.lounge);
    if (!lounge) throw new Error('missing lounge');
    lounge.textInVoice = false;
    initServer(snap);

    const { queryByTestId } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    expect(queryByTestId('voice-chat-button')).toBeNull();
  });
});
