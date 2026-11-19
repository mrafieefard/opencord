import { fireEvent, render, waitFor } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { AttachmentKind, AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';
import { useVideoPlayer } from 'expo-video';
import { router } from 'expo-router';

import { setCore } from '@/core';
import { FakeCore } from '@/core/fake/FakeCore';
import { mockAudioPlayer } from '@/test/setup';

import { AttachmentView } from '../AttachmentView';
import { VideoAttachment } from '../VideoAttachment';
import { AudioFileAttachment } from '../AudioFileAttachment';
import { FileAttachment } from '../FileAttachment';
import { isTextPreviewable } from '../previewable';
import { clearAttachmentCache } from '../useAttachmentFile';

jest.mock('expo-router', () => ({
  router: { push: jest.fn(), back: jest.fn() },
}));

describe('Media attachments and previews (Task 5)', () => {
  const serverKey = 'test-server';

  beforeEach(() => {
    jest.clearAllMocks();
    clearAttachmentCache();
  });

  // Fails if a video starts playing before its file is fully downloaded.
  it('a video plays only from a whole file', async () => {
    let resolveDownload: (path: string) => void = () => {};
    const downloadPromise = new Promise<string>((resolve) => {
      resolveDownload = resolve;
    });

    const fakeCore = new FakeCore({ scenario: 'empty' });
    let requestedVariant: AttachmentVariant | null = null;
    jest.spyOn(fakeCore, 'attachmentFile').mockImplementation(async (_sk, _id, variant) => {
      requestedVariant = variant;
      if (variant === AttachmentVariant.Original) {
        return downloadPromise;
      }
      return '/fake/attachments/vid-1.t';
    });
    setCore(fakeCore);

    const attachment: AttachmentRecord = {
      id: 'vid-1',
      filename: 'sample.mp4',
      contentType: 'video/mp4',
      sizeBytes: 5_000_000,
      kind: AttachmentKind.Video,
      durationMs: 45_000,
      width: 1920,
      height: 1080,
      hasThumbnail: true,
      animated: true,
      waveform: [],
    };

    const { getByTestId, queryByTestId } = await render(
      <GestureHandlerRootView>
        <VideoAttachment maxWidth={300} serverKey={serverKey} attachment={attachment} />
      </GestureHandlerRootView>,
    );

    // Initial state: poster tile with play button
    const playBtn = getByTestId('video-play-vid-1');
    expect(playBtn).toBeTruthy();
    // Video player should NOT be created or rendered yet
    expect(queryByTestId('video-player-modal-vid-1')).toBeNull();

    // Tap to play -> starts downloading original file
    fireEvent.press(playBtn);
    expect(requestedVariant).toBe(AttachmentVariant.Original);

    // While download is pending (only partial/downloading), player still must NOT be present
    await waitFor(() => {
      expect(getByTestId('video-loading-vid-1')).toBeTruthy();
    });
    expect(queryByTestId('video-player-modal-vid-1')).toBeNull();

    // Now whole file finishes downloading
    resolveDownload('/fake/attachments/vid-1.mp4');

    // Once whole file is ready, the player is active
    await waitFor(
      () => {
        expect(getByTestId('video-player-modal-vid-1')).toBeTruthy();
      },
      { timeout: 5000 },
    );
  });

  // Fails if multiple videos in a channel instantiate multiple VideoViews in the list.
  it('only the playing video has a player', async () => {
    const fakeCore = new FakeCore({ scenario: 'empty' });
    jest.spyOn(fakeCore, 'attachmentFile').mockImplementation(async (_sk, id, _variant) => {
      return `/fake/attachments/${id}.mp4`;
    });
    setCore(fakeCore);

    const video1: AttachmentRecord = {
      id: 'vid-play-1',
      filename: 'clip1.mp4',
      contentType: 'video/mp4',
      sizeBytes: 1_000_000,
      kind: AttachmentKind.Video,
      durationMs: 15_000,
      hasThumbnail: false,
      animated: true,
      waveform: [],
    };

    const video2: AttachmentRecord = {
      id: 'vid-play-2',
      filename: 'clip2.mp4',
      contentType: 'video/mp4',
      sizeBytes: 2_000_000,
      kind: AttachmentKind.Video,
      durationMs: 25_000,
      hasThumbnail: false,
      animated: true,
      waveform: [],
    };

    (useVideoPlayer as unknown as jest.Mock).mockClear();

    const { getByTestId, queryByTestId } = await render(
      <GestureHandlerRootView>
        <AttachmentView maxWidth={300} serverKey={serverKey} attachment={video1} />
        <AttachmentView maxWidth={300} serverKey={serverKey} attachment={video2} />
      </GestureHandlerRootView>,
    );

    // Both videos are in poster state (0 players instantiated)
    expect(getByTestId('video-play-vid-play-1')).toBeTruthy();
    expect(getByTestId('video-play-vid-play-2')).toBeTruthy();
    expect(queryByTestId('video-player-modal-vid-play-1')).toBeNull();
    expect(queryByTestId('video-player-modal-vid-play-2')).toBeNull();
    expect(useVideoPlayer).toHaveBeenCalledTimes(0);

    // Tap video 1 to play
    fireEvent.press(getByTestId('video-play-vid-play-1'));

    await waitFor(
      () => {
        expect(getByTestId('video-player-modal-vid-play-1')).toBeTruthy();
      },
      { timeout: 5000 },
    );
    // Only video 1 has instantiated a player (exactly 1 player)
    expect(useVideoPlayer).toHaveBeenCalledTimes(1);
    expect(queryByTestId('video-player-modal-vid-play-2')).toBeNull();
  });

  // Fails if non-text files or text files > 256 KB are offered preview.
  it('preview is offered only for text up to 256 KB', async () => {
    const fakeCore = new FakeCore({ scenario: 'empty' });
    setCore(fakeCore);

    // 1. Text file <= 256 KB
    const smallText: AttachmentRecord = {
      id: 'txt-1',
      filename: 'notes.txt',
      contentType: 'text/plain',
      sizeBytes: 10 * 1024,
      kind: AttachmentKind.File,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    // 2. Exactly 256 KB
    const maxText: AttachmentRecord = {
      id: 'txt-2',
      filename: 'code.rs',
      contentType: 'text/rust',
      sizeBytes: 256 * 1024,
      kind: AttachmentKind.File,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    // 3. Text file > 256 KB
    const hugeText: AttachmentRecord = {
      id: 'txt-3',
      filename: 'large.log',
      contentType: 'text/plain',
      sizeBytes: 256 * 1024 + 1,
      kind: AttachmentKind.File,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    // 4. Non-text file
    const binaryFile: AttachmentRecord = {
      id: 'bin-1',
      filename: 'app.zip',
      contentType: 'application/zip',
      sizeBytes: 50 * 1024,
      kind: AttachmentKind.File,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    expect(isTextPreviewable(smallText.filename, smallText.sizeBytes)).toBe(true);
    expect(isTextPreviewable(maxText.filename, maxText.sizeBytes)).toBe(true);
    expect(isTextPreviewable(hugeText.filename, hugeText.sizeBytes)).toBe(false);
    expect(isTextPreviewable(binaryFile.filename, binaryFile.sizeBytes)).toBe(false);

    const { getByTestId, queryByTestId } = await render(
      <GestureHandlerRootView>
        <FileAttachment maxWidth={300} serverKey={serverKey} attachment={smallText} />
        <FileAttachment maxWidth={300} serverKey={serverKey} attachment={hugeText} />
      </GestureHandlerRootView>,
    );

    // Small text offers Preview button
    const previewBtn = getByTestId('file-preview-txt-1');
    expect(previewBtn).toBeTruthy();
    fireEvent.press(previewBtn);
    expect(router.push).toHaveBeenCalledWith(
      expect.objectContaining({
        pathname: '/text-preview',
        params: expect.objectContaining({ serverKey, attachmentId: 'txt-1' }),
      }),
    );

    // Huge text file does NOT offer Preview button
    expect(queryByTestId('file-preview-txt-3')).toBeNull();
  });

  // Fails if an audio file does not play or seek through expo-audio.
  it('an audio file plays and seeks', async () => {
    const fakeCore = new FakeCore({ scenario: 'empty' });
    jest.spyOn(fakeCore, 'attachmentFile').mockImplementation(async () => {
      return '/fake/attachments/song.mp3';
    });
    setCore(fakeCore);

    const audioAttachment: AttachmentRecord = {
      id: 'aud-1',
      filename: 'song.mp3',
      contentType: 'audio/mpeg',
      sizeBytes: 3_500_000,
      kind: AttachmentKind.Audio,
      durationMs: 60_000,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    const { getByTestId } = await render(
      <GestureHandlerRootView>
        <AudioFileAttachment maxWidth={300} serverKey={serverKey} attachment={audioAttachment} />
      </GestureHandlerRootView>,
    );

    const playBtn = getByTestId('audio-play-aud-1');
    expect(playBtn).toBeTruthy();

    // Tap play
    fireEvent.press(playBtn);
    await waitFor(() => {
      expect(mockAudioPlayer.replace).toHaveBeenCalledWith({ uri: 'file:///fake/attachments/song.mp3' });
      expect(mockAudioPlayer.play).toHaveBeenCalledTimes(1);
    });

    // Seek scrubber through real accessibility action
    const scrubber = getByTestId('audio-scrubber-aud-1');
    expect(scrubber).toBeTruthy();
    fireEvent(scrubber, 'accessibilityAction', { nativeEvent: { actionName: 'increment' } });
    expect(mockAudioPlayer.seekTo).toHaveBeenCalledWith(5);
  });
});
