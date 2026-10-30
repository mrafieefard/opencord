// The core's events applied to the state, purely (design §6). Ready and the
// held events around it are store.ts's; everything else is here.
import {
  ConnectionRecord,
  MobileEvent,
  MobileEvent_Tags,
  UploadPhaseRecord_Tags,
  type ChannelAccess,
  type MessageRecord,
  type ServerEntry,
  type VoiceStateRecord,
} from '@opencord/core-native';

import {
  channelKey,
  compareIds,
  type AppState,
  type ChannelActivity,
  type ChannelMessages,
  type ServerState,
} from './types';

export const TYPING_MS = 10_000;
export const EMPTY_MESSAGES: ChannelMessages = Object.freeze({
  items: [],
  hasOlder: true,
  loadingOlder: false,
  loaded: false,
  pending: [],
}) as ChannelMessages;

export function newServer(entry: ServerEntry): ServerState {
  return {
    key: entry.key,
    entry,
    // Until the core says otherwise.
    connection: new ConnectionRecord.Connecting(),
    snapshot: null,
    seeding: false,
    readyCount: 0,
    held: [],
    activity: {},
    typing: {},
    voice: {},
    nowPlaying: {},
    musicSettings: null,
  };
}

function server(state: AppState, serverKey: string): ServerState | undefined {
  return state.servers[serverKey];
}

function withServer(state: AppState, serverKey: string, change: (server: ServerState) => ServerState): AppState {
  const current = server(state, serverKey);
  if (!current) return state;
  return { ...state, servers: { ...state.servers, [serverKey]: change(current) } };
}

/** Merges messages into a list ascending by id, each once. */
export function mergeMessages(items: MessageRecord[], incoming: MessageRecord[]): MessageRecord[] {
  const byId = new Map(items.map((message) => [message.id, message]));
  for (const message of incoming) byId.set(message.id, message);
  return [...byId.values()].sort((a, b) => compareIds(a.id, b.id));
}

function mentions(message: MessageRecord, selfId: string | undefined): boolean {
  return selfId !== undefined && message.content.includes(`<@${selfId}>`);
}

/** Whether the person is looking at the bottom of this channel right now. */
function reading(state: AppState, serverKey: string, channelId: string): boolean {
  const open = state.openChannel;
  return (
    state.appActive && open !== null && open.serverKey === serverKey && open.channelId === channelId && open.atBottom
  );
}

function messageCreated(state: AppState, serverKey: string, message: MessageRecord): AppState {
  const key = channelKey(serverKey, message.channelId);
  const loaded = state.messages[key];
  let next = state;
  if (loaded) {
    const pending = loaded.pending.filter((send) => send.nonce !== message.nonce || message.nonce === undefined);
    next = {
      ...next,
      messages: { ...next.messages, [key]: { ...loaded, items: mergeMessages(loaded.items, [message]), pending } },
    };
  }
  return withServer(next, serverKey, (current) => {
    const selfId = current.snapshot?.selfUser.id;
    const activity: ChannelActivity = current.activity[message.channelId] ?? { lastReadId: '0', unread: 0, mentions: 0 };
    // Known already (a seeding fetch had it): counted once.
    if (activity.last && compareIds(message.id, activity.last.id) <= 0) return current;
    const counts = message.authorId !== selfId && !reading(state, serverKey, message.channelId);
    const typing = { ...current.typing };
    if (typing[message.channelId]?.[message.authorId] !== undefined) {
      const { [message.authorId]: _gone, ...rest } = typing[message.channelId] ?? {};
      typing[message.channelId] = rest;
    }
    return {
      ...current,
      typing,
      activity: {
        ...current.activity,
        [message.channelId]: {
          last: message,
          lastReadId: counts ? activity.lastReadId : message.id,
          unread: counts ? activity.unread + 1 : 0,
          mentions: counts && mentions(message, selfId) ? activity.mentions + 1 : counts ? activity.mentions : 0,
        },
      },
    };
  });
}

function messageUpdated(state: AppState, serverKey: string, message: MessageRecord): AppState {
  const key = channelKey(serverKey, message.channelId);
  const loaded = state.messages[key];
  let next = state;
  if (loaded && loaded.items.some((item) => item.id === message.id)) {
    next = {
      ...next,
      messages: {
        ...next.messages,
        [key]: { ...loaded, items: loaded.items.map((item) => (item.id === message.id ? message : item)) },
      },
    };
  }
  return withServer(next, serverKey, (current) => {
    const activity = current.activity[message.channelId];
    if (activity?.last?.id !== message.id) return current;
    return { ...current, activity: { ...current.activity, [message.channelId]: { ...activity, last: message } } };
  });
}

function messageDeleted(state: AppState, serverKey: string, channelId: string, messageId: string): AppState {
  const key = channelKey(serverKey, channelId);
  const loaded = state.messages[key];
  let next = state;
  let items: MessageRecord[] = [];
  if (loaded) {
    items = loaded.items.filter((item) => item.id !== messageId);
    next = { ...next, messages: { ...next.messages, [key]: { ...loaded, items } } };
  }
  return withServer(next, serverKey, (current) => {
    const activity = current.activity[channelId];
    if (activity?.last?.id !== messageId) return current;
    return { ...current, activity: { ...current.activity, [channelId]: { ...activity, last: items.at(-1) } } };
  });
}

function channelDeleted(state: AppState, serverKey: string, channelId: string): AppState {
  const { [channelKey(serverKey, channelId)]: _gone, ...messages } = state.messages;
  return withServer({ ...state, messages }, serverKey, (current) => {
    const { [channelId]: _activity, ...activity } = current.activity;
    const snapshot = current.snapshot && {
      ...current.snapshot,
      channels: current.snapshot.channels.filter((channel) => channel.id !== channelId),
      access: current.snapshot.access.filter((access) => access.channelId !== channelId),
    };
    return { ...current, snapshot, activity };
  });
}

function voiceStateChanged(state: AppState, serverKey: string, voiceState: VoiceStateRecord): AppState {
  let next = withServer(state, serverKey, (current) => {
    const { [voiceState.userId]: _old, ...others } = current.voice;
    return { ...current, voice: voiceState.channelId === undefined ? others : { ...others, [voiceState.userId]: voiceState } };
  });
  if (voiceState.thisDevice) {
    next =
      voiceState.channelId === undefined
        ? { ...next, myVoice: null }
        : {
            ...next,
            myVoice: {
              serverKey,
              channelId: voiceState.channelId,
              link: next.myVoice?.channelId === voiceState.channelId ? next.myVoice.link : null,
              muted: voiceState.selfMute,
              deafened: voiceState.selfDeaf,
            },
          };
  }
  return next;
}

function accessChanged(state: AppState, serverKey: string, access: ChannelAccess[]): AppState {
  return withServer(state, serverKey, (current) =>
    current.snapshot ? { ...current, snapshot: { ...current.snapshot, access } } : current,
  );
}

/** Applies events that aren't Ready, in order. */
export function applyEvents(state: AppState, events: MobileEvent[], nowMs: number): AppState {
  let next = state;
  for (const event of events) next = applyEvent(next, event, nowMs);
  return next;
}

function applyEvent(state: AppState, event: MobileEvent, nowMs: number): AppState {
  switch (event.tag) {
    case MobileEvent_Tags.Connection:
      return withServer(state, event.inner.serverKey, (current) => ({ ...current, connection: event.inner.state }));
    case MobileEvent_Tags.Ready:
      return withServer(state, event.inner.snapshot.serverKey, (current) => ({ ...current, snapshot: event.inner.snapshot }));
    case MobileEvent_Tags.MessageCreated:
      return messageCreated(state, event.inner.serverKey, event.inner.message);
    case MobileEvent_Tags.MessageUpdated:
      return messageUpdated(state, event.inner.serverKey, event.inner.message);
    case MobileEvent_Tags.MessageDeleted:
      return messageDeleted(state, event.inner.serverKey, event.inner.channelId, event.inner.messageId);
    case MobileEvent_Tags.ChannelUpserted:
      return withServer(state, event.inner.serverKey, (current) => {
        if (!current.snapshot) return current;
        const channels = current.snapshot.channels.filter((channel) => channel.id !== event.inner.channel.id);
        return { ...current, snapshot: { ...current.snapshot, channels: [...channels, event.inner.channel] } };
      });
    case MobileEvent_Tags.ChannelDeleted:
      return channelDeleted(state, event.inner.serverKey, event.inner.channelId);
    case MobileEvent_Tags.AccessChanged:
      return accessChanged(state, event.inner.serverKey, [...event.inner.access]);
    case MobileEvent_Tags.MemberUpserted:
      return withServer(state, event.inner.serverKey, (current) => {
        if (!current.snapshot) return current;
        const members = current.snapshot.members.filter((member) => member.user.id !== event.inner.member.user.id);
        return { ...current, snapshot: { ...current.snapshot, members: [...members, event.inner.member] } };
      });
    case MobileEvent_Tags.MemberLeft:
      return withServer(state, event.inner.serverKey, (current) => {
        if (!current.snapshot) return current;
        const members = current.snapshot.members.filter((member) => member.user.id !== event.inner.userId);
        return { ...current, snapshot: { ...current.snapshot, members } };
      });
    case MobileEvent_Tags.Typing:
      return withServer(state, event.inner.serverKey, (current) => ({
        ...current,
        typing: {
          ...current.typing,
          [event.inner.channelId]: { ...current.typing[event.inner.channelId], [event.inner.userId]: nowMs + TYPING_MS },
        },
      }));
    case MobileEvent_Tags.ServerInfoChanged:
      return withServer(state, event.inner.serverKey, (current) => ({
        ...current,
        entry: { ...current.entry, name: event.inner.name },
        snapshot: current.snapshot && { ...current.snapshot, name: event.inner.name, iconHash: event.inner.iconHash },
      }));
    case MobileEvent_Tags.VoiceStateChanged:
      return voiceStateChanged(state, event.inner.serverKey, event.inner.state);
    case MobileEvent_Tags.VoiceLink: {
      const mine = state.myVoice;
      if (!mine || mine.serverKey !== event.inner.serverKey || mine.channelId !== event.inner.channelId) return state;
      return { ...state, myVoice: { ...mine, link: event.inner.state } };
    }
    case MobileEvent_Tags.Speaking: {
      const key = channelKey(event.inner.serverKey, event.inner.channelId);
      const speaking = new Set(state.speaking[key] ?? []);
      for (const change of event.inner.changes) {
        if (change.speaking) speaking.add(change.userId);
        else speaking.delete(change.userId);
      }
      return { ...state, speaking: { ...state.speaking, [key]: [...speaking] } };
    }
    case MobileEvent_Tags.MusicSession: {
      const key = channelKey(event.inner.serverKey, event.inner.session.channelId);
      const held = state.music[key];
      if (held && held.version > event.inner.session.version) return state;
      return { ...state, music: { ...state.music, [key]: event.inner.session } };
    }
    case MobileEvent_Tags.MusicNowPlaying:
      return withServer(state, event.inner.serverKey, (current) => ({
        ...current,
        nowPlaying: { ...current.nowPlaying, [event.inner.nowPlaying.channelId]: event.inner.nowPlaying },
      }));
    case MobileEvent_Tags.MusicSettingsChanged:
      return withServer(state, event.inner.serverKey, (current) => ({ ...current, musicSettings: event.inner.settings }));
    case MobileEvent_Tags.SoundboardSettingsChanged:
      return withServer(state, event.inner.serverKey, (current) =>
        current.snapshot ? { ...current, snapshot: { ...current.snapshot, soundboard: event.inner.settings } } : current,
      );
    case MobileEvent_Tags.SoundboardSoundUpserted:
      return withServer(state, event.inner.serverKey, (current) => {
        if (!current.snapshot) return current;
        const sounds = current.snapshot.soundboardSounds.filter((sound) => sound.id !== event.inner.sound.id);
        return { ...current, snapshot: { ...current.snapshot, soundboardSounds: [...sounds, event.inner.sound] } };
      });
    case MobileEvent_Tags.SoundboardSoundDeleted:
      return withServer(state, event.inner.serverKey, (current) => {
        if (!current.snapshot) return current;
        const sounds = current.snapshot.soundboardSounds.filter((sound) => sound.id !== event.inner.soundId);
        return { ...current, snapshot: { ...current.snapshot, soundboardSounds: sounds } };
      });
    case MobileEvent_Tags.SoundPlayed: {
      // Timed here: the server's clock may not be this device's.
      const { effect } = event.inner;
      const key = channelKey(event.inner.serverKey, effect.channelId);
      return {
        ...state,
        soundRings: { ...state.soundRings, [key]: { ...state.soundRings[key], [effect.userId]: nowMs + effect.durationMs } },
      };
    }
    case MobileEvent_Tags.MusicUpload: {
      const held = state.uploads[event.inner.uploadId];
      return {
        ...state,
        uploads: {
          ...state.uploads,
          [event.inner.uploadId]: {
            uploadId: event.inner.uploadId,
            serverKey: event.inner.serverKey,
            channelId: event.inner.channelId,
            filename: event.inner.filename,
            sentBytes: event.inner.sentBytes,
            totalBytes: event.inner.totalBytes,
            phase: event.inner.phase,
            path: held?.path ?? '',
          },
        },
      };
    }
    case MobileEvent_Tags.SendProgress: {
      const { nonce, sentBytes, totalBytes } = event.inner;
      let changed = false;
      const nextMessages = { ...state.messages };
      for (const [k, channelMsgs] of Object.entries(nextMessages)) {
        if (channelMsgs.pending.some((p) => p.nonce === nonce)) {
          changed = true;
          nextMessages[k] = {
            ...channelMsgs,
            pending: channelMsgs.pending.map((p) =>
              p.nonce === nonce ? { ...p, progress: { sentBytes, totalBytes } } : p,
            ),
          };
          break;
        }
      }
      return changed ? { ...state, messages: nextMessages } : state;
    }
    case MobileEvent_Tags.AudioProblem:
    case MobileEvent_Tags.SpeakingWhileMuted:
    case MobileEvent_Tags.MusicNotice:
    case MobileEvent_Tags.Notice:
      // Shown, not kept: store.ts turns these into toasts.
      return state;
  }
}

/** Whether an upload has ended, one way or another. */
export function uploadEnded(tag: UploadPhaseRecord_Tags): boolean {
  return tag === UploadPhaseRecord_Tags.Done || tag === UploadPhaseRecord_Tags.Failed || tag === UploadPhaseRecord_Tags.Cancelled;
}
