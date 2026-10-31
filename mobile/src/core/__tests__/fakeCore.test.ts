import {
  MobileEvent,
  MusicAction,
  MusicPlayState,
  UploadPhaseRecord,
  UploadPhaseRecord_Tags,
  VoiceLinkState,
} from '@opencord/core-native';

import { FakeCore } from '../fake/FakeCore';
import { CHANNELS, NIGHT_OWLS } from '../fake/world';

async function drain(core: FakeCore): Promise<MobileEvent[]> {
  const generation = core.eventsAttach();
  const seen: MobileEvent[] = [];
  for (;;) {
    const batch = await Promise.race([
      core.nextEvents(generation, 256),
      new Promise<MobileEvent[]>((resolve) => setTimeout(() => resolve([]), 0)),
    ]);
    if (batch.length === 0) return seen;
    seen.push(...batch);
  }
}

describe('FakeCore answers as the core does', () => {
  it('a send echoes MessageCreated with its nonce and returns the same message', async () => {
    const core = new FakeCore();
    const sent = await core.messageSend(NIGHT_OWLS, CHANNELS.general, ' hello ', 'n1');
    const events = await drain(core);
    const echo = events.find(MobileEvent.MessageCreated.instanceOf);
    expect(sent.content).toBe('hello');
    expect(echo?.inner.message).toEqual(sent);
    expect(echo?.inner.message.nonce).toBe('n1');
  });

  it('fetch pages newest first, before an id', async () => {
    const core = new FakeCore();
    const newest = await core.messagesFetch(NIGHT_OWLS, CHANNELS.general, undefined, 3);
    expect(newest.map((message) => message.id)).toEqual(['100119', '100118', '100117']);
    const older = await core.messagesFetch(NIGHT_OWLS, CHANNELS.general, '100117', 2);
    expect(older.map((message) => message.id)).toEqual(['100116', '100115']);
  });

  it('uploads move through their phases, and fail.mp3 fails', async () => {
    jest.useFakeTimers();
    try {
      const core = new FakeCore({ uploadStepMs: 10 });
      const done = core.musicUploadSong(NIGHT_OWLS, CHANNELS.lounge, 'u1', '/cache/song.mp3', false);
      const failed = core.musicUploadSong(NIGHT_OWLS, CHANNELS.lounge, 'u2', '/cache/fail.mp3', false);
      failed.catch(() => {});
      await jest.advanceTimersByTimeAsync(100);
      await expect(done).resolves.toBeUndefined();
      await expect(failed).rejects.toMatchObject({ tag: 'Server' });
      jest.useRealTimers();
      const phases = (await drain(core))
        .filter(MobileEvent.MusicUpload.instanceOf)
        .map((event) => `${event.inner.uploadId}:${event.inner.phase.tag}`);
      expect(phases.filter((phase) => phase.startsWith('u1'))).toEqual([
        `u1:${UploadPhaseRecord_Tags.Preparing}`,
        `u1:${UploadPhaseRecord_Tags.Uploading}`,
        `u1:${UploadPhaseRecord_Tags.Uploading}`,
        `u1:${UploadPhaseRecord_Tags.Uploading}`,
        `u1:${UploadPhaseRecord_Tags.Processing}`,
        `u1:${UploadPhaseRecord_Tags.Done}`,
      ]);
      expect(phases.at(-1)).toBe(`u2:${new UploadPhaseRecord.Failed({ message: '' }).tag}`);
    } finally {
      jest.useRealTimers();
    }
  });

  it('playback changes the session and raises its version', async () => {
    const core = new FakeCore();
    const before = await core.musicSubscribe(NIGHT_OWLS, CHANNELS.lounge);
    const paused = await core.musicPlayback(NIGHT_OWLS, CHANNELS.lounge, MusicAction.Pause, undefined, undefined);
    expect(paused.state).toBe(MusicPlayState.Paused);
    expect(paused.version).toBe(before.version + 1);
    const sessions = (await drain(core)).filter(MobileEvent.MusicSession.instanceOf);
    expect(sessions.at(-1)?.inner.session).toEqual(paused);
  });

  it('joining voice sends VoiceStateChanged and VoiceLink Connected', async () => {
    const core = new FakeCore();
    await core.voiceJoin(NIGHT_OWLS, CHANNELS.lounge);
    const events = await drain(core);
    const state = events.find(MobileEvent.VoiceStateChanged.instanceOf);
    expect(state?.inner.state).toMatchObject({ thisDevice: true, channelId: CHANNELS.lounge });
    const links = events.filter(MobileEvent.VoiceLink.instanceOf).map((event) => event.inner.state.tag);
    expect(links.at(-1)).toBe(new VoiceLinkState.Connected().tag);
  });
});
