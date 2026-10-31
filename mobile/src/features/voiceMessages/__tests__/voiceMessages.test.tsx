import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { AttachmentKind, ConnectionRecord, MobileEvent, PlaybackItemRecord, type AttachmentRecord } from '@opencord/core-native';
import { setAudioModeAsync, setIsAudioActiveAsync } from 'expo-audio';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { VoiceMessageAttachment, clock, voiceWidth } from '@/features/chat/attachments/VoiceMessageAttachment';
import {
  cancelRecording,
  nextVoiceSpeed,
  sendVoiceDraft,
  startRecording,
  stopRecording,
} from '@/state/actions/voiceMessages';
import { EMPTY_MESSAGES } from '@/state/events';
import { onEvents, useAppStore } from '@/state/store';
import { channelKey } from '@/state/types';
import { freshStore, settle } from '@/test/store';

import { resample } from '../Waveform';

async function pump(core: FakeCore): Promise<void> {
  onEvents(await core.nextEvents(core.eventsAttach(), 100));
}

async function ready(): Promise<FakeCore> {
  const core = freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: { ...snapshot(NIGHT_OWLS), voiceMessageMaxSeconds: 120 } }),
  ]);
  await settle();
  return core;
}

const voice: AttachmentRecord = {
  id: '700001',
  filename: 'voice-message.ogg',
  contentType: 'audio/ogg',
  sizeBytes: 40_000,
  kind: AttachmentKind.VoiceMessage,
  durationMs: 13_000,
  waveform: Array.from({ length: 128 }, (_, index) => index * 2),
  hasThumbnail: false,
  animated: false,
};

describe('voice messages', () => {
  it('the waveform keeps the loudest level of each slice', () => {
    expect(resample([1, 9, 2, 3, 7, 4], 3)).toEqual([9, 3, 7]);
    expect(resample([5, 6], 4)).toEqual([5, 5, 6, 6]);
    expect(resample([], 2)).toEqual([0, 0]);
  });

  it('times and widths read as the design says', () => {
    expect(clock(7_400)).toBe('0:07');
    expect(clock(750_000)).toBe('12:30');
    expect(voiceWidth(5_000)).toBe(200);
    expect(voiceWidth(35_000)).toBe(260);
    expect(voiceWidth(90_000)).toBe(320);
  });

  it("records up to the server's maximum, with live levels, and sends one attachment", async () => {
    const core = await ready();
    const recordStart = jest.spyOn(core, 'voiceRecordStart');
    expect(await startRecording(NIGHT_OWLS)).toBe(true);
    expect(core.calls.find((call) => call.method === 'voiceRecordStart')?.args).toEqual([120]);
    // Outside a call it takes the audio session for itself, and turns it on
    // before the core opens the microphone: an inactive session has none.
    expect(setAudioModeAsync).toHaveBeenLastCalledWith(expect.objectContaining({ allowsRecording: true, interruptionMode: 'duckOthers' }));
    expect(setIsAudioActiveAsync).toHaveBeenLastCalledWith(true);
    const activated = jest.mocked(setIsAudioActiveAsync).mock.invocationCallOrder.at(-1) ?? Infinity;
    expect(activated).toBeLessThan(recordStart.mock.invocationCallOrder[0] ?? -1);

    onEvents([new MobileEvent.VoiceRecordLevel({ level: 0.4 }), new MobileEvent.VoiceRecordLevel({ level: 0.7 })]);
    expect(useAppStore.getState().voiceRecording?.levels).toEqual([0.4, 0.7]);

    const draft = await stopRecording();
    if (!draft) throw new Error('no draft');
    expect(draft.durationMs).toBe(2_000);
    expect(useAppStore.getState().voiceRecording).toBeNull();

    // The chat is open, its messages loaded.
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    useAppStore.setState((state) => ({ messages: { ...state.messages, [key]: { ...EMPTY_MESSAGES, loaded: true } } }));
    await sendVoiceDraft(NIGHT_OWLS, CHANNELS.general, draft);
    await pump(core);
    const sent = useAppStore.getState().messages[key]?.items.at(-1);
    expect(sent?.content).toBe('');
    expect(sent?.attachments).toEqual([expect.objectContaining({ kind: AttachmentKind.VoiceMessage, durationMs: 2_000 })]);
  });

  it('a cancelled recording keeps nothing', async () => {
    const core = await ready();
    await startRecording(NIGHT_OWLS);
    await cancelRecording();
    expect(core.calls.some((call) => call.method === 'voiceRecordCancel')).toBe(true);
    // The session goes off again: other apps play at full volume.
    expect(setIsAudioActiveAsync).toHaveBeenLastCalledWith(false);
    expect(useAppStore.getState().voiceRecording).toBeNull();
  });

  it('stopping by itself at the maximum is marked for the composer', async () => {
    await ready();
    await startRecording(NIGHT_OWLS);
    onEvents([new MobileEvent.VoiceRecordStopped({ maxLength: true })]);
    expect(useAppStore.getState().voiceRecording?.stopped).toBe('maxLength');
  });

  it('a bubble plays, shows where it is, and changes speed', async () => {
    const core = await ready();
    await render(
      <GestureHandlerRootView>
        <VoiceMessageAttachment serverKey={NIGHT_OWLS} attachment={voice} maxWidth={300} />
      </GestureHandlerRootView>,
    );
    expect(screen.getByText('0:13')).toBeTruthy();
    expect(screen.getByText('1×')).toBeTruthy();

    await act(async () => {
      fireEvent.press(screen.getByLabelText('Play voice message'));
    });
    await pump(core);
    const played = core.calls.find((call) => call.method === 'voicePlay')?.args;
    expect(PlaybackItemRecord.Attachment.instanceOf(played?.[0])).toBe(true);
    expect(played?.slice(1)).toEqual([0, 1]);

    onEvents([
      new MobileEvent.Playback({
        item: new PlaybackItemRecord.Attachment({ serverKey: NIGHT_OWLS, attachmentId: voice.id }),
        positionMs: 5_000,
        durationMs: 13_000,
        playing: true,
        buffering: false,
      }),
    ]);
    expect(await screen.findByText('0:05 / 0:13')).toBeTruthy();
    expect(screen.getByLabelText('Pause')).toBeTruthy();

    await act(async () => {
      nextVoiceSpeed();
    });
    expect(screen.getByText('1.5×')).toBeTruthy();
    expect(core.calls.find((call) => call.method === 'voiceSetSpeed')?.args).toEqual([1.5]);
  });
});
