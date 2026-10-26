import {
  ChannelKind,
  ConnectionRecord,
  MobileEvent,
  MusicPlayState,
  UploadPhaseRecord,
  VoiceLinkState,
  type MessageRecord,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, fullAccess, session, snapshot } from '@/core/fake/world';
import { freshStore } from '@/test/store';

import { applyEvents, TYPING_MS } from '../events';
import { useAppStore } from '../store';
import { channelKey, type AppState } from '../types';

const now = 1_761_500_000_000;
const message = (id: string, channelId: string = CHANNELS.general, authorId = '1001'): MessageRecord => ({
  id,
  channelId,
  authorId,
  content: `m${id}`,
  createdAtMs: now,
  attachments: [],
});

function ready(): AppState {
  freshStore();
  return applyEvents(useAppStore.getState(), [new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })], now);
}

const apply = (state: AppState, ...events: MobileEvent[]) => applyEvents(state, events, now);

describe('applying events', () => {
  it('ready replaces everything known of a server', () => {
    const state = ready();
    const changed = apply(state, new MobileEvent.Ready({ snapshot: { ...snapshot(NIGHT_OWLS), name: 'Owls' } }));
    expect(changed.servers[NIGHT_OWLS]?.snapshot?.name).toBe('Owls');
  });

  it('channel events patch the snapshot, and a deleted channel loses its activity and messages', () => {
    let state = ready();
    state = apply(
      state,
      new MobileEvent.ChannelUpserted({
        serverKey: NIGHT_OWLS,
        channel: { id: '99', kind: ChannelKind.Text, name: 'new', position: 9, userLimit: 0, textInVoice: false },
      }),
    );
    expect(state.servers[NIGHT_OWLS]?.snapshot?.channels.some((channel) => channel.id === '99')).toBe(true);
    state = {
      ...state,
      messages: { [channelKey(NIGHT_OWLS, '99')]: { items: [message('1', '99')], hasOlder: false, loadingOlder: false, loaded: true, pending: [] } },
    };
    state = apply(state, new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: message('2', '99') }));
    state = apply(state, new MobileEvent.ChannelDeleted({ serverKey: NIGHT_OWLS, channelId: '99' }));
    expect(state.servers[NIGHT_OWLS]?.snapshot?.channels.some((channel) => channel.id === '99')).toBe(false);
    expect(state.servers[NIGHT_OWLS]?.activity['99']).toBeUndefined();
    expect(state.messages[channelKey(NIGHT_OWLS, '99')]).toBeUndefined();
  });

  it("access changes replace the channel's flags", () => {
    const state = apply(
      ready(),
      new MobileEvent.AccessChanged({ serverKey: NIGHT_OWLS, access: [{ ...fullAccess(CHANNELS.general), send: false }] }),
    );
    expect(state.servers[NIGHT_OWLS]?.snapshot?.access).toEqual([{ ...fullAccess(CHANNELS.general), send: false }]);
  });

  it('members join, change and leave', () => {
    let state = ready();
    const joined = { user: { id: '2000', displayName: 'Zoe', fingerprint: 'Z' }, roleIds: [] };
    state = apply(state, new MobileEvent.MemberUpserted({ serverKey: NIGHT_OWLS, member: joined }));
    state = apply(state, new MobileEvent.MemberUpserted({ serverKey: NIGHT_OWLS, member: { ...joined, nickname: 'Z' } }));
    const members = () => state.servers[NIGHT_OWLS]?.snapshot?.members.filter((member) => member.user.id === '2000') ?? [];
    expect(members()).toEqual([{ ...joined, nickname: 'Z' }]);
    state = apply(state, new MobileEvent.MemberLeft({ serverKey: NIGHT_OWLS, userId: '2000' }));
    expect(members()).toEqual([]);
  });

  it('voice states track who is where, and leaving removes them', () => {
    let state = ready();
    const voice = { userId: '1003', channelId: CHANNELS.lounge, thisDevice: false, selfMute: false, selfDeaf: false, serverMute: false, serverDeaf: false };
    state = apply(state, new MobileEvent.VoiceStateChanged({ serverKey: NIGHT_OWLS, state: voice }));
    expect(state.servers[NIGHT_OWLS]?.voice['1003']).toEqual(voice);
    state = apply(state, new MobileEvent.VoiceStateChanged({ serverKey: NIGHT_OWLS, state: { ...voice, channelId: undefined } }));
    expect(state.servers[NIGHT_OWLS]?.voice['1003']).toBeUndefined();
  });

  it('speaking follows the changes', () => {
    let state = ready();
    const speaking = (userId: string, on: boolean) =>
      new MobileEvent.Speaking({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, changes: [{ userId, speaking: on }] });
    state = apply(state, speaking('1001', true), speaking('1002', true), speaking('1001', false));
    expect(state.speaking[channelKey(NIGHT_OWLS, CHANNELS.lounge)]).toEqual(['1002']);
  });

  it('a music session with a lower version is ignored', () => {
    let state = ready();
    const newer = { ...session(CHANNELS.lounge, now), version: 5 };
    state = apply(state, new MobileEvent.MusicSession({ serverKey: NIGHT_OWLS, session: newer }));
    state = apply(state, new MobileEvent.MusicSession({ serverKey: NIGHT_OWLS, session: { ...newer, version: 4, state: MusicPlayState.Paused } }));
    expect(state.music[channelKey(NIGHT_OWLS, CHANNELS.lounge)]).toEqual(newer);
  });

  it("typing ends after 10 s or with that user's message", () => {
    let state = ready();
    state = applyEvents(state, [new MobileEvent.Typing({ serverKey: NIGHT_OWLS, channelId: CHANNELS.general, userId: '1001' })], now);
    expect(state.servers[NIGHT_OWLS]?.typing[CHANNELS.general]?.['1001']).toBe(now + TYPING_MS);
    state = apply(state, new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: message('5') }));
    expect(state.servers[NIGHT_OWLS]?.typing[CHANNELS.general]?.['1001']).toBeUndefined();
  });

  it("my voice follows the link and ends with this device's empty voice state", () => {
    let state = ready();
    const mine = { userId: SELF_ID, channelId: CHANNELS.lounge, thisDevice: true, selfMute: true, selfDeaf: false, serverMute: false, serverDeaf: false };
    state = apply(
      state,
      new MobileEvent.VoiceStateChanged({ serverKey: NIGHT_OWLS, state: mine }),
      new MobileEvent.VoiceLink({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, state: new VoiceLinkState.Connected() }),
    );
    expect(state.myVoice).toMatchObject({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, muted: true });
    expect(state.myVoice?.link?.tag).toBe('Connected');
    state = apply(state, new MobileEvent.VoiceStateChanged({ serverKey: NIGHT_OWLS, state: { ...mine, channelId: undefined } }));
    expect(state.myVoice).toBeNull();
  });

  it('upload progress updates its row, and done marks it queued', () => {
    let state = ready();
    const upload = (phase: UploadPhaseRecord, sentBytes: number) =>
      new MobileEvent.MusicUpload({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, uploadId: 'u1', filename: 'a.mp3', sentBytes, totalBytes: 10, phase });
    state = apply(state, upload(new UploadPhaseRecord.Uploading(), 4));
    expect(state.uploads.u1).toMatchObject({ sentBytes: 4, phase: { tag: 'Uploading' } });
    state = apply(state, upload(new UploadPhaseRecord.Done(), 10));
    expect(state.uploads.u1?.phase.tag).toBe('Done');
  });

  it('deleting the last message falls back to the newest loaded one, or none', () => {
    let state = ready();
    state = {
      ...state,
      messages: { [channelKey(NIGHT_OWLS, CHANNELS.offTopic)]: { items: [], hasOlder: false, loadingOlder: false, loaded: true, pending: [] } },
    };
    state = apply(
      state,
      new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: message('7', CHANNELS.offTopic) }),
      new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: message('8', CHANNELS.offTopic) }),
      new MobileEvent.MessageDeleted({ serverKey: NIGHT_OWLS, channelId: CHANNELS.offTopic, messageId: '8' }),
    );
    expect(state.servers[NIGHT_OWLS]?.activity[CHANNELS.offTopic]?.last?.id).toBe('7');
    state = apply(state, new MobileEvent.MessageDeleted({ serverKey: NIGHT_OWLS, channelId: CHANNELS.offTopic, messageId: '7' }));
    expect(state.servers[NIGHT_OWLS]?.activity[CHANNELS.offTopic]?.last).toBeUndefined();
  });

  it('a connection state is kept for its server', () => {
    const state = apply(ready(), new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Reconnecting({ attempt: 1, retryInMs: 1000 }) }));
    expect(state.servers[NIGHT_OWLS]?.connection.tag).toBe('Reconnecting');
  });
});
