// The app's state (design §6). Records come from the core as they are.
import type {
  ChannelAccess,
  ChannelRecord,
  ConnectionRecord,
  MessageRecord,
  MobileEvent,
  MusicNowPlayingRecord,
  MusicSessionRecord,
  MusicSettingsRecord,
  ServerEntry,
  ServerSnapshot,
  UploadPhaseRecord,
  VoiceLinkState,
  VoiceStateRecord,
} from '@opencord/core-native';

import type { SelfInfo } from '@/core/boot';

export interface ChannelActivity {
  /** The newest message known. */
  last?: MessageRecord;
  /** Read up to here; "0" for a channel started empty. */
  lastReadId: string;
  unread: number;
  mentions: number;
}

export interface ServerState {
  key: string;
  entry: ServerEntry;
  connection: ConnectionRecord;
  /** From the latest Ready, patched by events since. */
  snapshot: ServerSnapshot | null;
  /** Recent activity is being fetched after a Ready; events wait in `held`. */
  seeding: boolean;
  /** Counts Readys, so an older seeding's result is thrown away. */
  readyCount: number;
  held: MobileEvent[];
  activity: Record<string, ChannelActivity>;
  /** channel → user → when it ends (ms). */
  typing: Record<string, Record<string, number>>;
  /** Everyone in a voice channel. */
  voice: Record<string, VoiceStateRecord>;
  nowPlaying: Record<string, MusicNowPlayingRecord>;
  musicSettings: MusicSettingsRecord | null;
}

export interface PickedFile {
  path: string;
  filename: string;
  size: number;
  mimeType: string;
}

export interface PendingSend {
  nonce: string;
  serverKey: string;
  channelId: string;
  content: string;
  files: PickedFile[];
  state: 'sending' | 'failed';
  error?: string;
  createdAtMs: number;
  progress?: { sentBytes: number; totalBytes: number };
}

export interface EditingMessage {
  serverKey: string;
  channelId: string;
  messageId: string;
  text: string;
}

export interface ChannelMessages {
  /** Ascending by id, each once. */
  items: MessageRecord[];
  hasOlder: boolean;
  loadingOlder: boolean;
  loaded: boolean;
  pending: PendingSend[];
}

export interface MyVoice {
  serverKey: string;
  channelId: string;
  link: VoiceLinkState | null;
  muted: boolean;
  deafened: boolean;
}

export interface MusicUploadState {
  uploadId: string;
  serverKey: string;
  channelId: string;
  filename: string;
  sentBytes: number;
  totalBytes: number;
  phase: UploadPhaseRecord;
  /** The picker's copy, deleted once the upload ends. */
  path: string;
}

export type Theme = 'system' | 'light' | 'dark';

export interface MusicListening {
  volume: number;
  muted: boolean;
  duck: boolean;
}

export interface Prefs {
  theme: Theme;
  music: MusicListening;
  /** Soundboard favourites, by picker.ts's favoriteKey. */
  soundFavorites: string[];
}

export interface Toast {
  id: number;
  text: string;
  tone: 'neutral' | 'error';
}

export type BootState =
  | { state: 'starting' }
  | { state: 'onboarding' }
  | { state: 'ready' }
  | { state: 'failed'; message: string };

export interface OpenChannel {
  serverKey: string;
  channelId: string;
  atBottom: boolean;
}

export interface AppState {
  boot: BootState;
  self: SelfInfo | null;
  servers: Record<string, ServerState>;
  /** As the core lists them. */
  order: string[];
  /** Keyed `serverKey/channelId`. */
  messages: Record<string, ChannelMessages>;
  openChannel: OpenChannel | null;
  appActive: boolean;
  myVoice: MyVoice | null;
  /** Keyed `serverKey/channelId`: who speaks now. */
  speaking: Record<string, string[]>;
  /** Keyed `serverKey/channelId`. */
  music: Record<string, MusicSessionRecord>;
  uploads: Record<string, MusicUploadState>;
  /** Keyed `serverKey/userId`: how someone sounds here, as handed to the core (which keeps it in memory too). */
  userAudio: Record<string, UserAudio>;
  /** Keyed `serverKey/channelId`: who played a sound, and when its ring ends (ms, this device's clock). */
  soundRings: Record<string, Record<string, number>>;
  /** When this device last played a sound, per server: the cooldown runs from it. */
  lastSound: Record<string, number>;
  prefs: Prefs;
  toasts: Toast[];
  editingMessage: EditingMessage | null;
}

/** A row of the server list (design §8.2). */
export interface ServerRow {
  key: string;
  name: string;
  address: string;
  iconHash?: string;
  connection: ConnectionRecord;
  preview: { channelName: string; authorName: string; text: string; own: boolean } | null;
  lastAtMs: number | null;
  unread: number;
  mentions: number;
  inVoice: boolean;
}

/** How loud someone is on this device (spec §8.6, their sheet). */
export interface UserAudio {
  /** 0–2, 1 being as sent. */
  volume: number;
  mutedForMe: boolean;
}

export interface VoiceMember {
  userId: string;
  name: string;
  avatarHash?: string;
  muted: boolean;
  deafened: boolean;
  speaking: boolean;
  isSelf: boolean;
}

export type ChannelItem =
  | { kind: 'text'; channel: ChannelRecord; access: ChannelAccess; unread: number; mentions: number }
  | {
      kind: 'voice';
      channel: ChannelRecord;
      access: ChannelAccess;
      members: VoiceMember[];
      nowPlaying: MusicNowPlayingRecord | null;
      count: number;
      limit: number;
    };

export interface ChannelSection {
  category: ChannelRecord | null;
  items: ChannelItem[];
}

export function channelKey(serverKey: string, channelId: string): string {
  return `${serverKey}/${channelId}`;
}

/** Ids are decimal strings of 64-bit numbers: compare them as numbers. */
export function compareIds(a: string, b: string): number {
  const left = BigInt(a);
  const right = BigInt(b);
  return left < right ? -1 : left > right ? 1 : 0;
}
