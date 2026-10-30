import {
  ChannelKind,
  ConnectionRecord,
  MobileEvent,
  UploadPhaseRecord,
  VoiceLinkState,
  type ChannelAccess,
  type ChannelRecord,
} from '@opencord/core-native';
import { act, fireEvent, render } from '@testing-library/react-native';
import * as DocumentPicker from 'expo-document-picker';
import { File } from 'expo-file-system';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { getCore } from '@/core';
import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore } from '@/test/store';

import { AddBar } from '../AddBar';
import { Uploads } from '../Uploads';

/** The player's upload button and the rows above Up next. */
function UploadArea({ serverKey, channelId }: { serverKey: string; channelId: string }) {
  return (
    <>
      <AddBar serverKey={serverKey} channelId={channelId} onSearch={() => {}} />
      <Uploads serverKey={serverKey} channelId={channelId} />
    </>
  );
}

async function renderSection(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

describe('Uploads', () => {
  const channelId = '20'; // Lounge

  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
    jest.useRealTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  function setupServer(inVoice = true, musicAdd = true, attach = true, musicManage = false) {
    const s = snapshot(NIGHT_OWLS);
    const channel: ChannelRecord = {
      id: channelId,
      kind: ChannelKind.Voice,
      name: 'Lounge',
      position: 0,
      parentId: undefined,
      topic: undefined,
      userLimit: 0,
      textInVoice: false,
    };
    const access: ChannelAccess = {
      channelId,
      view: true,
      readHistory: true,
      send: true,
      attach,
      connect: true,
      speak: true,
      manageMessages: false,
      musicAdd,
      musicControl: true,
      musicManageQueue: true,
      musicManage,
      useSoundboard: true,
      useExternalSounds: true,
    };
    useAppStore.setState({
      servers: {
        [NIGHT_OWLS]: {
          key: NIGHT_OWLS,
          entry: {
            key: NIGHT_OWLS,
            name: 'Night Owls',
            host: 'night-owls.example',
            port: 7710,
            userId: '1000',
          },
          connection: new ConnectionRecord.Connected(),
          snapshot: {
            ...s,
            channels: [channel],
            access: [access],
          },
          musicSettings: null,
          seeding: false,
          readyCount: 1,
          held: [],
          activity: {},
          typing: {},
          voice: {},
          nowPlaying: {},
        },
      },
      myVoice: inVoice
        ? {
            serverKey: NIGHT_OWLS,
            channelId,
            link: new VoiceLinkState.Connected(),
            muted: false,
            deafened: false,
          }
        : null,
    });
  }

  it('without permission the upload button says what is missing', async () => {
    // User has musicAdd and attach, but is not in the voice channel
    setupServer(false, true, true, false);
    const pickSpy = jest.spyOn(DocumentPicker, 'getDocumentAsync');

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    await act(async () => {
      fireEvent.press(view.getByLabelText('Upload audio files'));
    });

    expect(useAppStore.getState().toasts.some((t) => t.text === 'Join Lounge to upload songs.')).toBe(true);
    expect(pickSpy).not.toHaveBeenCalled();
  });

  it('with Manage music the upload button works outside the channel', async () => {
    setupServer(false, false, false, true);
    const pickSpy = jest
      .spyOn(DocumentPicker, 'getDocumentAsync')
      .mockResolvedValue({ canceled: true, assets: null } as unknown as DocumentPicker.DocumentPickerResult);

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    await act(async () => {
      fireEvent.press(view.getByLabelText('Upload audio files'));
    });

    expect(pickSpy).toHaveBeenCalled();
  });

  it('picking files starts one upload per file with its own id', async () => {
    setupServer(true);
    const mockUploadSong = jest.spyOn(getCore(), 'musicUploadSong').mockResolvedValue(undefined);

    jest.spyOn(DocumentPicker, 'getDocumentAsync').mockResolvedValue({
      canceled: false,
      assets: [
        {
          name: 'track1.mp3',
          uri: 'file:///cache/track1.mp3',
          mimeType: 'audio/mpeg',
          size: 4_000_000,
        },
        {
          name: 'track 2.flac',
          uri: 'file:///cache/track%202.flac',
          mimeType: 'audio/flac',
          size: 8_000_000,
        },
      ],
    } as unknown as DocumentPicker.DocumentPickerResult);

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    const chooseButton = view.getByLabelText('Upload audio files');
    await act(async () => {
      fireEvent.press(chooseButton);
    });

    expect(mockUploadSong).toHaveBeenCalledTimes(2);
    expect(mockUploadSong).toHaveBeenNthCalledWith(
      1,
      NIGHT_OWLS,
      channelId,
      expect.any(String),
      // The core opens plain paths.
      '/cache/track1.mp3',
      false,
    );
    expect(mockUploadSong).toHaveBeenNthCalledWith(
      2,
      NIGHT_OWLS,
      channelId,
      expect.any(String),
      '/cache/track 2.flac',
      false,
    );

    const id1 = mockUploadSong.mock.calls[0]?.[2];
    const id2 = mockUploadSong.mock.calls[1]?.[2];
    expect(id1).not.toBe(id2);
  });

  it('rows follow the phases', async () => {
    setupServer(true);
    useAppStore.setState({
      uploads: {
        u1: {
          uploadId: 'u1',
          serverKey: NIGHT_OWLS,
          channelId,
          filename: 'mysong.mp3',
          sentBytes: 0,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Preparing(),
          path: 'file:///cache/mysong.mp3',
        },
      },
    });

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    expect(view.getByText('mysong.mp3')).toBeTruthy();
    expect(view.getByText('Checking…')).toBeTruthy();

    // Update to Uploading
    await act(async () => {
      onEvents([
        new MobileEvent.MusicUpload({
          serverKey: NIGHT_OWLS,
          channelId,
          uploadId: 'u1',
          filename: 'mysong.mp3',
          sentBytes: 2_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Uploading(),
        }),
      ]);
    });
    expect(await view.findByText('Uploading · 50 % of 3.8 MB')).toBeTruthy();

    // Update to Processing
    await act(async () => {
      onEvents([
        new MobileEvent.MusicUpload({
          serverKey: NIGHT_OWLS,
          channelId,
          uploadId: 'u1',
          filename: 'mysong.mp3',
          sentBytes: 4_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Processing(),
        }),
      ]);
    });
    expect(await view.findByText('The server is checking it…')).toBeTruthy();

    // Update to Done
    await act(async () => {
      onEvents([
        new MobileEvent.MusicUpload({
          serverKey: NIGHT_OWLS,
          channelId,
          uploadId: 'u1',
          filename: 'mysong.mp3',
          sentBytes: 4_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Done(),
        }),
      ]);
    });
    expect(await view.findByText('Queued')).toBeTruthy();
  });

  it('cancel stops an upload', async () => {
    setupServer(true);
    const mockCancelSong = jest.spyOn(getCore(), 'musicCancelSong');
    useAppStore.setState({
      uploads: {
        u1: {
          uploadId: 'u1',
          serverKey: NIGHT_OWLS,
          channelId,
          filename: 'mysong.mp3',
          sentBytes: 1_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Uploading(),
          path: 'file:///cache/mysong.mp3',
        },
      },
    });

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    const cancelBtn = await view.findByLabelText('Cancel upload mysong.mp3');
    fireEvent.press(cancelBtn);

    expect(mockCancelSong).toHaveBeenCalledWith(NIGHT_OWLS, 'u1');
  });

  it('a failure shows the server reason and can be retried', async () => {
    setupServer(true);
    const mockUploadSong = jest.spyOn(getCore(), 'musicUploadSong').mockResolvedValue(undefined);
    useAppStore.setState({
      uploads: {
        u1: {
          uploadId: 'u1',
          serverKey: NIGHT_OWLS,
          channelId,
          filename: 'corrupted.mp3',
          sentBytes: 4_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Failed({ message: 'Corrupted audio stream' }),
          path: 'file:///cache/corrupted.mp3',
        },
      },
    });

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    expect(await view.findByText('Failed: Corrupted audio stream')).toBeTruthy();

    const retryBtn = await view.findByLabelText('Retry corrupted.mp3');
    await act(async () => {
      fireEvent.press(retryBtn);
    });

    expect(mockUploadSong).toHaveBeenCalledWith(
      NIGHT_OWLS,
      channelId,
      'u1',
      'file:///cache/corrupted.mp3',
      false,
    );
  });

  it('a queued row fades within 3 s and its copy is deleted', async () => {
    jest.useFakeTimers();
    setupServer(true);
    const deleteSpy = jest.spyOn(File.prototype, 'delete');

    useAppStore.setState({
      uploads: {
        u1: {
          uploadId: 'u1',
          serverKey: NIGHT_OWLS,
          channelId,
          filename: 'mysong.mp3',
          sentBytes: 4_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Done(),
          path: 'file:///cache/mysong.mp3',
        },
      },
    });

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    expect(await view.findByText('Queued')).toBeTruthy();

    await act(async () => {
      jest.advanceTimersByTime(2700);
    });
    await act(async () => {
      jest.advanceTimersByTime(300);
    });

    expect(deleteSpy).toHaveBeenCalled();
    expect(useAppStore.getState().uploads.u1).toBeUndefined();
  });

  it('dismissing a failed row deletes its copy', async () => {
    setupServer(true);
    const deleteSpy = jest.spyOn(File.prototype, 'delete');

    useAppStore.setState({
      uploads: {
        u1: {
          uploadId: 'u1',
          serverKey: NIGHT_OWLS,
          channelId,
          filename: 'bad.mp3',
          sentBytes: 4_000_000,
          totalBytes: 4_000_000,
          phase: new UploadPhaseRecord.Failed({ message: 'Too big' }),
          path: 'file:///cache/bad.mp3',
        },
      },
    });

    const view = await renderSection(<UploadArea serverKey={NIGHT_OWLS} channelId={channelId} />);
    const dismissBtn = await view.findByLabelText('Dismiss bad.mp3');
    fireEvent.press(dismissBtn);

    expect(deleteSpy).toHaveBeenCalled();
    expect(useAppStore.getState().uploads.u1).toBeUndefined();
  });
});

