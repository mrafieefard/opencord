// How components read the store (plan rule 4): small slices with stable
// references, and derived views built in useMemo from them.
import {
  AttachmentKind,
  ChannelKind,
  ConnectionRecord_Tags,
  type ChannelAccess,
  type ChannelRecord,
  type MusicNowPlayingRecord,
  type MusicSessionRecord,
  type MusicSettingsRecord,
  type ServerSnapshot,
} from '@opencord/core-native';
import { useEffect, useMemo, useState } from 'react';

import { soundboardPicker, type Picker } from '@/features/soundboard/picker';

import { EMPTY_MESSAGES } from './events';
import { previewText } from './preview';
import { defaultSounds } from './actions/soundboard';
import { DEFAULT_USER_AUDIO, useAppStore } from './store';
import {
  channelKey,
  compareIds,
  type AppState,
  type ChannelActivity,
  type ChannelItem,
  type ChannelMessages,
  type ChannelSection,
  type MusicUploadState,
  type MyVoice,
  type ServerRow,
  type ServerState,
  type UserAudio,
  type VoiceMember,
} from './types';

export function memberName(snapshot: ServerSnapshot | null, userId: string): string | undefined {
  const member = snapshot?.members.find((candidate) => candidate.user.id === userId);
  return member ? (member.nickname ?? member.user.displayName) : undefined;
}

function channelName(snapshot: ServerSnapshot | null, channelId: string): string | undefined {
  return snapshot?.channels.find((channel) => channel.id === channelId)?.name;
}

function serverRow(server: ServerState, myVoice: MyVoice | null): ServerRow {
  const snapshot = server.snapshot;
  let latest: ChannelActivity | undefined;
  let unread = 0;
  let mentions = 0;
  for (const activity of Object.values(server.activity)) {
    unread += activity.unread;
    mentions += activity.mentions;
    if (activity.last && (!latest?.last || compareIds(activity.last.id, latest.last.id) > 0)) latest = activity;
  }
  const last = latest?.last;
  const selfId = snapshot?.selfUser.id;
  const lookup = {
    member: (id: string) => memberName(snapshot, id),
    channel: (id: string) => channelName(snapshot, id),
  };
  const text = last
    ? previewText(last.content, lookup) ||
      (last.attachments.some((attachment) => attachment.kind === AttachmentKind.Image) ? 'Photo' : last.attachments.length ? 'File' : '')
    : '';
  return {
    key: server.key,
    name: snapshot?.name ?? server.entry.name,
    address: `${server.entry.host}:${server.entry.port}`,
    iconHash: snapshot?.iconHash,
    connection: server.connection,
    preview: last
      ? {
          channelName: channelName(snapshot, last.channelId) ?? '',
          authorName: last.authorId === selfId ? 'You' : (memberName(snapshot, last.authorId) ?? 'Someone'),
          text,
          own: last.authorId === selfId,
        }
      : null,
    lastAtMs: last ? last.createdAtMs : null,
    unread,
    mentions,
    inVoice: myVoice?.serverKey === server.key,
  };
}

/** Latest activity first; servers with none after, by name. */
export function buildServerRows(state: Pick<AppState, 'servers' | 'order' | 'myVoice'>, query: string): ServerRow[] {
  const words = query.trim().toLowerCase();
  return state.order
    .flatMap((key) => {
      const server = state.servers[key];
      return server ? [serverRow(server, state.myVoice)] : [];
    })
    .filter((row) => words === '' || row.name.toLowerCase().includes(words))
    .sort((a, b) => {
      if (a.lastAtMs !== null && b.lastAtMs !== null) return b.lastAtMs - a.lastAtMs;
      if (a.lastAtMs !== null) return -1;
      if (b.lastAtMs !== null) return 1;
      return a.name.localeCompare(b.name);
    });
}

export function buildVoiceMembers(
  server: ServerState,
  channelId: string,
  speaking: readonly string[],
): VoiceMember[] {
  const selfId = server.snapshot?.selfUser.id;
  return Object.values(server.voice)
    .filter((state) => state.channelId === channelId)
    .map((state) => {
      const member = server.snapshot?.members.find((candidate) => candidate.user.id === state.userId);
      return {
        userId: state.userId,
        name: member ? (member.nickname ?? member.user.displayName) : 'Someone',
        avatarHash: member?.user.avatarHash,
        muted: state.selfMute || state.serverMute,
        deafened: state.selfDeaf || state.serverDeaf,
        speaking: speaking.includes(state.userId),
        isSelf: state.userId === selfId,
      };
    })
    .sort((a, b) => Number(b.isSelf) - Number(a.isSelf) || a.name.localeCompare(b.name));
}

const byPosition = (a: ChannelRecord, b: ChannelRecord) => a.position - b.position || compareIds(a.id, b.id);

/** Channels without a category first, then each category by position. */
export function buildChannelSections(
  server: ServerState,
  speaking: Readonly<Record<string, string[]>>,
): ChannelSection[] {
  const snapshot = server.snapshot;
  if (!snapshot) return [];
  const access = new Map<string, ChannelAccess>(snapshot.access.map((entry) => [entry.channelId, entry]));
  const item = (channel: ChannelRecord): ChannelItem | null => {
    const allowed = access.get(channel.id);
    if (!allowed?.view) return null;
    if (channel.kind === ChannelKind.Voice) {
      const members = buildVoiceMembers(server, channel.id, speaking[channelKey(server.key, channel.id)] ?? []);
      const nowPlaying: MusicNowPlayingRecord | null = server.nowPlaying[channel.id] ?? null;
      return {
        kind: 'voice',
        channel,
        access: allowed,
        members,
        nowPlaying: nowPlaying && !nowPlaying.ended ? nowPlaying : null,
        count: members.length,
        limit: channel.userLimit,
      };
    }
    const activity = server.activity[channel.id];
    return { kind: 'text', channel, access: allowed, unread: activity?.unread ?? 0, mentions: activity?.mentions ?? 0 };
  };
  const items = (parentId: string | undefined) =>
    snapshot.channels
      .filter((channel) => channel.kind !== ChannelKind.Category && channel.parentId === parentId)
      .sort(byPosition)
      .flatMap((channel) => item(channel) ?? []);
  const sections: ChannelSection[] = [{ category: null, items: items(undefined) }];
  for (const category of snapshot.channels.filter((channel) => channel.kind === ChannelKind.Category).sort(byPosition)) {
    sections.push({ category, items: items(category.id) });
  }
  return sections.filter((section) => section.items.length > 0);
}

export function typingNames(server: ServerState | undefined, channelId: string, nowMs: number): string[] {
  const typing = server?.typing[channelId] ?? {};
  const selfId = server?.snapshot?.selfUser.id;
  return Object.entries(typing)
    .filter(([userId, until]) => until > nowMs && userId !== selfId)
    .map(([userId]) => memberName(server?.snapshot ?? null, userId) ?? 'Someone')
    .sort();
}

// Hooks --------------------------------------------------------------------

export const useBoot = () => useAppStore((state) => state.boot);
export const useSelf = () => useAppStore((state) => state.self);
export const usePrefs = () => useAppStore((state) => state.prefs);
export const useToasts = () => useAppStore((state) => state.toasts);
export const useMyVoice = (): MyVoice | null => useAppStore((state) => state.myVoice);
export const useServer = (serverKey: string): ServerState | undefined =>
  useAppStore((state) => state.servers[serverKey]);

export function useServerRows(query: string): ServerRow[] {
  const servers = useAppStore((state) => state.servers);
  const order = useAppStore((state) => state.order);
  const myVoice = useAppStore((state) => state.myVoice);
  return useMemo(() => buildServerRows({ servers, order, myVoice }, query), [servers, order, myVoice, query]);
}

export function useServerHeader(serverKey: string) {
  const server = useServer(serverKey);
  return useMemo(
    () =>
      server
        ? {
            name: server.snapshot?.name ?? server.entry.name,
            iconHash: server.snapshot?.iconHash,
            connection: server.connection,
            connected: server.connection.tag === ConnectionRecord_Tags.Connected,
            address: `${server.entry.host}:${server.entry.port}`,
          }
        : null,
    [server],
  );
}

export function useChannelSections(serverKey: string): ChannelSection[] {
  const server = useServer(serverKey);
  const speaking = useAppStore((state) => state.speaking);
  return useMemo(() => (server ? buildChannelSections(server, speaking) : []), [server, speaking]);
}

export function useChannelInfo(serverKey: string, channelId: string): { channel: ChannelRecord; access: ChannelAccess } | null {
  const server = useServer(serverKey);
  return useMemo(() => {
    const channel = server?.snapshot?.channels.find((candidate) => candidate.id === channelId);
    const access = server?.snapshot?.access.find((candidate) => candidate.channelId === channelId);
    return channel && access ? { channel, access } : null;
  }, [server, channelId]);
}

export const useChannelActivity = (serverKey: string, channelId: string): ChannelActivity | undefined =>
  useAppStore((state) => state.servers[serverKey]?.activity[channelId]);

export const useChannelMessages = (serverKey: string, channelId: string): ChannelMessages =>
  useAppStore((state) => state.messages[channelKey(serverKey, channelId)] ?? EMPTY_MESSAGES);

export function useTypingNames(serverKey: string, channelId: string, nowMs: number): string[] {
  const server = useServer(serverKey);
  return useMemo(() => typingNames(server, channelId, nowMs), [server, channelId, nowMs]);
}

export function useMember(serverKey: string, userId: string) {
  const server = useServer(serverKey);
  return useMemo(() => {
    const member = server?.snapshot?.members.find((candidate) => candidate.user.id === userId);
    return member
      ? { userId, name: member.nickname ?? member.user.displayName, avatarHash: member.user.avatarHash }
      : null;
  }, [server, userId]);
}

export function useMemberName(serverKey: string, userId: string): string {
  return useMember(serverKey, userId)?.name ?? 'Someone';
}

export function useVoiceMembers(serverKey: string, channelId: string): VoiceMember[] {
  const server = useServer(serverKey);
  const speaking = useAppStore((state) => state.speaking[channelKey(serverKey, channelId)]);
  return useMemo(() => (server ? buildVoiceMembers(server, channelId, speaking ?? []) : []), [server, channelId, speaking]);
}

export const useNowPlaying = (serverKey: string, channelId: string): MusicNowPlayingRecord | null =>
  useAppStore((state) => state.servers[serverKey]?.nowPlaying[channelId] ?? null);

export const useMusicSession = (serverKey: string, channelId: string): MusicSessionRecord | null =>
  useAppStore((state) => state.music[channelKey(serverKey, channelId)] ?? null);

/** How someone sounds on this device; the core's defaults until changed. */
export const useUserAudio = (serverKey: string, userId: string): UserAudio =>
  useAppStore((state) => state.userAudio[channelKey(serverKey, userId)] ?? DEFAULT_USER_AUDIO);

export const useMusicSettings = (serverKey: string): MusicSettingsRecord | null =>
  useAppStore((state) => state.servers[serverKey]?.musicSettings ?? null);

export function useMusicUploads(serverKey: string, channelId: string): MusicUploadState[] {
  const uploads = useAppStore((state) => state.uploads);
  return useMemo(
    () => Object.values(uploads).filter((upload) => upload.serverKey === serverKey && upload.channelId === channelId),
    [uploads, serverKey, channelId],
  );
}

/** The soundboard for this device's voice channel, searched; null outside voice. */
export function useSoundboardPicker(query: string): Picker | null {
  const myVoice = useAppStore((state) => state.myVoice);
  const servers = useAppStore((state) => state.servers);
  const order = useAppStore((state) => state.order);
  const prefs = useAppStore((state) => state.prefs);
  return useMemo(() => soundboardPicker({ myVoice, servers, order, prefs }, defaultSounds(), query), [myVoice, servers, order, prefs, query]);
}

/** Whether someone's soundboard ring shows: until their sound ends. */
export function useSoundRing(serverKey: string, channelId: string, userId: string): boolean {
  const endsAt = useAppStore((state) => state.soundRings[channelKey(serverKey, channelId)]?.[userId]);
  // The end that has passed; a newer sound brings a newer end.
  const [ended, setEnded] = useState<number>();
  useEffect(() => {
    if (endsAt === undefined) return;
    const timer = setTimeout(() => setEnded(endsAt), Math.max(endsAt - Date.now(), 0));
    return () => clearTimeout(timer);
  }, [endsAt]);
  return endsAt !== undefined && ended !== endsAt;
}

/** When this device last played a sound on a server, for the cooldown. */
export const useLastSound = (serverKey: string): number | undefined => useAppStore((state) => state.lastSound[serverKey]);
