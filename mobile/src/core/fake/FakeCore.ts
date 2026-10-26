// A core with no Rust behind it, for tests and for screenshots
// (EXPO_PUBLIC_FAKE_CORE=1). It answers as the core does: the same errors
// with the same words, the echo of a send with its nonce, the events the
// core sends after each call (plan rule 11).
import {
  AddServerResult,
  AttachmentKind,
  AttachmentVariant,
  ConnectionRecord,
  FailureKind,
  MobileError,
  MobileEvent,
  MusicAction,
  MusicPlacement,
  MusicPlayState,
  UploadPhaseRecord,
  VoiceLinkState,
  type IdentityInfo,
  type MessageRecord,
  type MusicQueueItemRecord,
  type MusicRepeat,
  type MusicSearchResultsRecord,
  type MusicSessionRecord,
  type MusicSettingsRecord,
  type NewIdentity,
  type OutgoingFileRecord,
  type ServerEntry,
  type UserRecord,
  type VoiceStateRecord,
} from '@opencord/core-native';

import type { CoreApi } from '../api';
import {
  CHANNELS,
  NIGHT_OWLS,
  RUST_CLUB,
  SELF_ID,
  WEEKEND_RAID,
  catalogue,
  entries,
  history,
  item,
  musicSettings,
  session,
  snapshot,
  track,
  user,
} from './world';

export type Scenario = 'default' | 'empty';

export interface FakeOptions {
  now?: () => number;
  scenario?: Scenario;
  /** A file for an attachment; tests leave the default path. */
  files?: (attachmentId: string, variant: AttachmentVariant) => Promise<string>;
  /** Time between an upload's steps. */
  uploadStepMs?: number;
  /** Speaking that changes by itself, for the running app. */
  live?: boolean;
}

interface Waiter {
  generation: bigint;
  max: number;
  resolve: (events: MobileEvent[]) => void;
}

interface Upload {
  serverKey: string;
  cancelled: boolean;
}

const SECRET_LEN = 32;
const MESSAGE_MAX_CHARS = 4000;
const NAME_MAX_CHARS = 32;
const BACKUP_PREFIX = 'opencord-identity-v1';

function invalid(message: string): never {
  throw new MobileError.InvalidInput({ message });
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** The core's single-line name rule: trimmed, 1–32 characters, no controls. */
function checkName(name: string): string {
  const trimmed = name.trim();
  if (trimmed.length === 0) invalid('display name must not be empty');
  if (/[\u0000-\u001f\u007f-\u009f]/.test(trimmed)) invalid('display name must not contain control characters');
  if ([...trimmed].length > NAME_MAX_CHARS) invalid(`display name must be at most ${NAME_MAX_CHARS} characters`);
  return trimmed;
}

function checkSecret(secret: ArrayBuffer): Uint8Array {
  if (secret.byteLength !== SECRET_LEN) invalid('an identity secret is exactly 32 bytes');
  return new Uint8Array(secret);
}

function compareIds(a: string, b: string): number {
  const left = BigInt(a);
  const right = BigInt(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

export class FakeCore implements CoreApi {
  readonly calls: { method: keyof CoreApi; args: unknown[] }[] = [];
  readonly scenario: Scenario;
  private readonly now: () => number;
  private readonly files: FakeOptions['files'];
  private readonly uploadStepMs: number;
  private queue: MobileEvent[] = [];
  private generation = 0n;
  private waiter: Waiter | null = null;
  private failures = new Map<keyof CoreApi, unknown>();
  private settings = new Map<string, string>();
  private servers: ServerEntry[];
  private trusted = new Set<string>();
  private messages = new Map<string, MessageRecord[]>();
  private sessions = new Map<string, MusicSessionRecord>();
  private uploads = new Map<string, Upload>();
  private voice: VoiceStateRecord | null = null;
  private voiceServer: string | null = null;
  private nextId = 900_000;
  private name = 'Ava';
  private loaded = false;

  constructor(options: FakeOptions = {}) {
    this.now = options.now ?? Date.now;
    this.scenario = options.scenario ?? 'default';
    this.files = options.files;
    this.uploadStepMs = options.uploadStepMs ?? 400;
    this.servers = this.scenario === 'empty' ? [] : entries();
    if (this.scenario === 'default') {
      const now = this.now();
      this.messages.set(this.key(NIGHT_OWLS, CHANNELS.general), history(CHANNELS.general, 120, now, 100_000));
      this.messages.set(this.key(NIGHT_OWLS, CHANNELS.musicRequests), history(CHANNELS.musicRequests, 8, now - 3_600_000, 200_000));
      this.messages.set(this.key(NIGHT_OWLS, CHANNELS.offTopic), []);
      this.sessions.set(this.key(NIGHT_OWLS, CHANNELS.lounge), session(CHANNELS.lounge, now));
    }
    if (options.live) setInterval(() => this.toggleSpeaking(), 2000);
  }

  // Test hooks ---------------------------------------------------------

  /** Delivers events as the core would, in order. */
  emit(...events: MobileEvent[]): void {
    this.queue.push(...events);
    this.flush();
  }

  /** The next call of `method` fails with `error`. */
  failNext(method: keyof CoreApi, error: unknown): void {
    this.failures.set(method, error);
  }

  /** The identity a fake start uses (design: deep links open at once). */
  fakeIdentity(): { secret: ArrayBuffer; name: string } {
    return { secret: new Uint8Array(SECRET_LEN).fill(7).buffer, name: 'Ava' };
  }

  // Events -------------------------------------------------------------

  eventsAttach(): bigint {
    this.enter('eventsAttach', []);
    this.generation += 1n;
    if (this.waiter) {
      const waiter = this.waiter;
      this.waiter = null;
      waiter.resolve([]);
    }
    return this.generation;
  }

  nextEvents(generation: bigint, max: number): Promise<MobileEvent[]> {
    this.enter('nextEvents', [generation, max]);
    if (generation !== this.generation) return Promise.resolve([]);
    if (this.queue.length > 0) return Promise.resolve(this.queue.splice(0, max));
    return new Promise((resolve) => {
      this.waiter = { generation, max, resolve };
    });
  }

  private flush(): void {
    if (!this.waiter || this.queue.length === 0) return;
    const waiter = this.waiter;
    this.waiter = null;
    waiter.resolve(this.queue.splice(0, waiter.max));
  }

  // Setup and identity -------------------------------------------------

  coreInit(dataDir: string, cacheDir: string): void {
    this.enter('coreInit', [dataDir, cacheDir]);
  }

  coreVersion(): string {
    return '0.0.1-fake';
  }

  settingsGet(key: string): string | undefined {
    this.enter('settingsGet', [key]);
    return this.settings.get(key);
  }

  settingsSet(key: string, value: string | undefined): void {
    this.enter('settingsSet', [key, value]);
    if (value === undefined) this.settings.delete(key);
    else this.settings.set(key, value);
  }

  identityGenerate(): NewIdentity {
    this.enter('identityGenerate', []);
    const secret = new Uint8Array(SECRET_LEN);
    for (let index = 0; index < SECRET_LEN; index += 1) secret[index] = Math.floor(Math.random() * 256);
    return { secret: secret.buffer, info: this.info(secret) };
  }

  identityCheck(secret: ArrayBuffer, displayName: string): IdentityInfo {
    this.enter('identityCheck', [secret, displayName]);
    const bytes = checkSecret(secret);
    checkName(displayName);
    return this.info(bytes);
  }

  identityLoad(secret: ArrayBuffer, displayName: string): IdentityInfo {
    this.enter('identityLoad', [secret, displayName]);
    const bytes = checkSecret(secret);
    this.name = checkName(displayName);
    if (!this.loaded) {
      this.loaded = true;
      for (const server of this.servers) this.connect(server.key);
    }
    return this.info(bytes);
  }

  identityBackupEncode(secret: ArrayBuffer): string {
    this.enter('identityBackupEncode', [secret]);
    const bytes = checkSecret(secret);
    return `${BACKUP_PREFIX}:${hex(bytes)}:00000000`;
  }

  identityBackupDecode(text: string): ArrayBuffer {
    this.enter('identityBackupDecode', [text]);
    const rest = text.trim().startsWith(`${BACKUP_PREFIX}:`) ? text.trim().slice(BACKUP_PREFIX.length + 1) : null;
    if (rest === null) invalid('this is not an Opencord identity backup');
    const secretHex = rest.split(':')[0] ?? '';
    if (!/^[0-9a-f]{64}$/.test(secretHex)) invalid('the identity backup is damaged');
    const bytes = new Uint8Array(SECRET_LEN);
    for (let index = 0; index < SECRET_LEN; index += 1) bytes[index] = parseInt(secretHex.slice(index * 2, index * 2 + 2), 16);
    return bytes.buffer;
  }

  private info(secret: Uint8Array): IdentityInfo {
    const publicKeyHex = hex(secret);
    const short = publicKeyHex.slice(0, 16).toUpperCase();
    return { publicKeyHex, fingerprint: short.match(/.{4}/g)?.join('-') ?? short };
  }

  // Servers ------------------------------------------------------------

  serversList(): ServerEntry[] {
    this.enter('serversList', []);
    return [...this.servers];
  }

  async serverAdd(linkOrAddress: string, claimToken: string | undefined): Promise<AddServerResult> {
    this.enter('serverAdd', [linkOrAddress, claimToken]);
    const address = linkOrAddress.replace(/^opencords?:\/\//, '').split('/')[0] ?? linkOrAddress;
    if (!/^[^\s:]+:\d+$/.test(address)) invalid(`not an invite link or address: ${linkOrAddress}`);
    if (!this.trusted.has(address) && !linkOrAddress.includes('#fp=')) {
      return new AddServerResult.NeedsTrust({ address, fingerprint: 'ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12' });
    }
    const [host = address, port = '7710'] = address.split(':');
    const server: ServerEntry = { key: address, name: host, host, port: Number(port), userId: SELF_ID };
    this.servers = [...this.servers.filter((entry) => entry.key !== address), server];
    this.connect(address);
    return new AddServerResult.Added({ server });
  }

  serverTrust(address: string, fingerprint: string): void {
    this.enter('serverTrust', [address, fingerprint]);
    this.trusted.add(address);
  }

  async serverRetryNow(serverKey: string): Promise<void> {
    this.enter('serverRetryNow', [serverKey]);
    this.requireServer(serverKey);
  }

  async serverRemove(serverKey: string): Promise<void> {
    this.enter('serverRemove', [serverKey]);
    this.requireServer(serverKey);
    this.servers = this.servers.filter((entry) => entry.key !== serverKey);
  }

  async profileUpdate(serverKey: string, displayName: string): Promise<UserRecord> {
    this.enter('profileUpdate', [serverKey, displayName]);
    this.requireConnected(serverKey);
    return { ...user(SELF_ID), displayName: checkName(displayName) };
  }

  async avatarFile(serverKey: string, hash: string): Promise<string> {
    this.enter('avatarFile', [serverKey, hash]);
    return `/fake/avatars/${hash}.png`;
  }

  /** What the core sends as it connects: each fake server has its state. */
  private connect(serverKey: string): void {
    this.emit(new MobileEvent.Connection({ serverKey, state: new ConnectionRecord.Connecting() }));
    if (serverKey === RUST_CLUB) {
      this.emit(
        new MobileEvent.Connection({
          serverKey,
          state: new ConnectionRecord.Reconnecting({ attempt: 2, retryInMs: 5000 }),
        }),
      );
      return;
    }
    if (serverKey === WEEKEND_RAID) {
      this.emit(
        new MobileEvent.Connection({
          serverKey,
          state: new ConnectionRecord.Failed({
            reason: FailureKind.Rejected,
            message: 'not a member',
            expectedFingerprint: undefined,
            presentedFingerprint: undefined,
          }),
        }),
      );
      return;
    }
    this.emit(
      new MobileEvent.Connection({ serverKey, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: { ...snapshot(serverKey), selfUser: { ...user(SELF_ID), displayName: this.name } } }),
    );
  }

  private requireServer(serverKey: string): void {
    if (!this.servers.some((entry) => entry.key === serverKey)) throw new MobileError.UnknownServer();
  }

  private requireConnected(serverKey: string): void {
    this.requireServer(serverKey);
    if (serverKey === RUST_CLUB || serverKey === WEEKEND_RAID) throw new MobileError.NotConnected();
  }

  // Chat ---------------------------------------------------------------

  async messagesFetch(serverKey: string, channelId: string, before: string | undefined, limit: number): Promise<MessageRecord[]> {
    this.enter('messagesFetch', [serverKey, channelId, before, limit]);
    this.requireConnected(serverKey);
    const all = this.messages.get(this.key(serverKey, channelId)) ?? [];
    const older = before === undefined ? all : all.filter((message) => compareIds(message.id, before) < 0);
    return older.slice(-Math.min(Math.max(limit, 1), 100)).reverse();
  }

  async messageSend(serverKey: string, channelId: string, content: string, nonce: string): Promise<MessageRecord> {
    this.enter('messageSend', [serverKey, channelId, content, nonce]);
    this.requireConnected(serverKey);
    const text = content.trim();
    if (text.length === 0) invalid('message must not be empty');
    if ([...text].length > MESSAGE_MAX_CHARS) invalid(`message must be at most ${MESSAGE_MAX_CHARS} characters`);
    return this.post(serverKey, channelId, text, nonce, []);
  }

  async messageSendFiles(
    serverKey: string,
    channelId: string,
    content: string,
    nonce: string,
    files: OutgoingFileRecord[],
  ): Promise<MessageRecord> {
    this.enter('messageSendFiles', [serverKey, channelId, content, nonce, files]);
    this.requireConnected(serverKey);
    files.forEach((_, index) => {
      this.emit(
        new MobileEvent.SendProgress({ serverKey, nonce, index, sentBytes: 1000, totalBytes: 1000, phase: new UploadPhaseRecord.Done() }),
      );
    });
    const attachments = files.map((file) => {
      const filename = file.filename || (file.path.split('/').pop() ?? 'file');
      const image = /\.(png|jpe?g|gif|webp)$/i.test(filename);
      return {
        id: String(this.id()),
        filename,
        contentType: image ? 'image/jpeg' : 'application/octet-stream',
        sizeBytes: 1000,
        kind: image ? AttachmentKind.Image : AttachmentKind.File,
        width: image ? 1200 : undefined,
        height: image ? 900 : undefined,
        hasThumbnail: image,
        thumbnailWidth: image ? 400 : undefined,
        thumbnailHeight: image ? 300 : undefined,
        animated: false,
      };
    });
    return this.post(serverKey, channelId, content.trim(), nonce, attachments);
  }

  sendCancel(serverKey: string, nonce: string): void {
    this.enter('sendCancel', [serverKey, nonce]);
  }

  async messageEdit(serverKey: string, messageId: string, content: string): Promise<MessageRecord> {
    this.enter('messageEdit', [serverKey, messageId, content]);
    this.requireConnected(serverKey);
    const found = this.find(serverKey, messageId);
    const edited: MessageRecord = { ...found.message, content: content.trim(), editedAtMs: this.now() };
    found.list[found.index] = edited;
    this.emit(new MobileEvent.MessageUpdated({ serverKey, message: edited }));
    return edited;
  }

  async messageDelete(serverKey: string, messageId: string): Promise<void> {
    this.enter('messageDelete', [serverKey, messageId]);
    this.requireConnected(serverKey);
    const found = this.find(serverKey, messageId);
    found.list.splice(found.index, 1);
    this.emit(new MobileEvent.MessageDeleted({ serverKey, channelId: found.message.channelId, messageId }));
  }

  async typingStart(serverKey: string, channelId: string): Promise<void> {
    this.enter('typingStart', [serverKey, channelId]);
    this.requireConnected(serverKey);
  }

  async attachmentFile(serverKey: string, attachmentId: string, variant: AttachmentVariant): Promise<string> {
    this.enter('attachmentFile', [serverKey, attachmentId, variant]);
    if (this.files) return this.files(attachmentId, variant);
    return `/fake/attachments/${attachmentId}.${variant === AttachmentVariant.Original ? 'o' : 't'}`;
  }

  private post(
    serverKey: string,
    channelId: string,
    content: string,
    nonce: string,
    attachments: MessageRecord['attachments'],
  ): MessageRecord {
    const message: MessageRecord = {
      id: String(this.id()),
      channelId,
      authorId: SELF_ID,
      content,
      createdAtMs: this.now(),
      nonce,
      attachments,
    };
    const key = this.key(serverKey, channelId);
    this.messages.set(key, [...(this.messages.get(key) ?? []), message]);
    this.emit(new MobileEvent.MessageCreated({ serverKey, message }));
    return message;
  }

  private find(serverKey: string, messageId: string): { list: MessageRecord[]; index: number; message: MessageRecord } {
    for (const [key, list] of this.messages) {
      if (!key.startsWith(`${serverKey}/`)) continue;
      const index = list.findIndex((message) => message.id === messageId);
      const message = list[index];
      if (message) return { list, index, message };
    }
    throw new MobileError.Server({ code: 'not_found', message: 'no such message', retryAfterMs: undefined });
  }

  // Voice --------------------------------------------------------------

  async voiceJoin(serverKey: string, channelId: string): Promise<VoiceStateRecord> {
    this.enter('voiceJoin', [serverKey, channelId]);
    this.requireConnected(serverKey);
    if (this.voice && this.voiceServer) await this.voiceLeave();
    const state: VoiceStateRecord = {
      userId: SELF_ID,
      channelId,
      thisDevice: true,
      selfMute: false,
      selfDeaf: false,
      serverMute: false,
      serverDeaf: false,
    };
    this.voice = state;
    this.voiceServer = serverKey;
    this.emit(
      new MobileEvent.VoiceStateChanged({ serverKey, state }),
      new MobileEvent.VoiceLink({ serverKey, channelId, state: new VoiceLinkState.AwaitingEndpoint() }),
      new MobileEvent.VoiceLink({ serverKey, channelId, state: new VoiceLinkState.Connected() }),
    );
    return state;
  }

  async voiceLeave(): Promise<void> {
    this.enter('voiceLeave', []);
    if (!this.voice || !this.voiceServer) return;
    const state: VoiceStateRecord = { ...this.voice, channelId: undefined };
    const serverKey = this.voiceServer;
    this.voice = null;
    this.voiceServer = null;
    this.emit(new MobileEvent.VoiceStateChanged({ serverKey, state }));
  }

  voiceSetMute(muted: boolean): void {
    this.enter('voiceSetMute', [muted]);
    this.updateVoice({ selfMute: muted });
  }

  voiceSetDeaf(deafened: boolean): void {
    this.enter('voiceSetDeaf', [deafened]);
    this.updateVoice({ selfDeaf: deafened });
  }

  voiceSetUserVolume(serverKey: string, userId: string, volume: number): void {
    this.enter('voiceSetUserVolume', [serverKey, userId, volume]);
  }

  voiceSetUserLocalMute(serverKey: string, userId: string, muted: boolean): void {
    this.enter('voiceSetUserLocalMute', [serverKey, userId, muted]);
  }

  private updateVoice(change: Partial<VoiceStateRecord>): void {
    if (!this.voice || !this.voiceServer) return;
    this.voice = { ...this.voice, ...change };
    this.emit(new MobileEvent.VoiceStateChanged({ serverKey: this.voiceServer, state: this.voice }));
  }

  private speaking = false;
  private toggleSpeaking(): void {
    this.speaking = !this.speaking;
    this.emit(
      new MobileEvent.Speaking({
        serverKey: NIGHT_OWLS,
        channelId: CHANNELS.lounge,
        changes: [{ userId: '1001', speaking: this.speaking }],
      }),
    );
  }

  // Music --------------------------------------------------------------

  async musicSettings(serverKey: string): Promise<MusicSettingsRecord> {
    this.enter('musicSettings', [serverKey]);
    this.requireConnected(serverKey);
    return musicSettings();
  }

  async musicSubscribe(serverKey: string, channelId: string): Promise<MusicSessionRecord> {
    this.enter('musicSubscribe', [serverKey, channelId]);
    this.requireConnected(serverKey);
    return this.session(serverKey, channelId);
  }

  async musicUnsubscribe(serverKey: string, channelId: string): Promise<void> {
    this.enter('musicUnsubscribe', [serverKey, channelId]);
  }

  async musicSearch(serverKey: string, channelId: string, query: string, source: string): Promise<MusicSearchResultsRecord> {
    this.enter('musicSearch', [serverKey, channelId, query, source]);
    this.requireConnected(serverKey);
    const words = query.trim().toLowerCase();
    if (words.length === 0 || words.length > 200) {
      throw new MobileError.Server({ code: 'invalid_argument', message: 'a search is 1 to 200 characters', retryAfterMs: undefined });
    }
    const found = catalogue
      .map((track, index) => ({ resultId: `r-${index}`, track }))
      .filter(({ track }) => `${track.title} ${track.artist}`.toLowerCase().includes(words))
      .sort((a, b) => Number(a.track.source === 'local') - Number(b.track.source === 'local'));
    return { results: found, playlist: undefined };
  }

  async musicEnqueue(
    serverKey: string,
    channelId: string,
    resultIds: string[],
    historyItemIds: string[],
    beforeItemId: string | undefined,
    placement: MusicPlacement,
  ): Promise<MusicSessionRecord> {
    this.enter('musicEnqueue', [serverKey, channelId, resultIds, historyItemIds, beforeItemId, placement]);
    const current = this.session(serverKey, channelId);
    const fromResults = resultIds.flatMap((resultId) => {
      const track = catalogue[Number(resultId.replace('r-', ''))];
      return track ? [item(track, SELF_ID, this.now())] : [];
    });
    const fromHistory = historyItemIds.flatMap((itemId) => {
      const old = current.history.find((entry) => entry.itemId === itemId);
      return old ? [item(old.track, SELF_ID, this.now())] : [];
    });
    const added = [...fromResults, ...fromHistory];
    let queue = [...current.queue];
    if (placement === MusicPlacement.Next) queue = [...added, ...queue];
    else if (placement === MusicPlacement.Now) {
      const [first, ...rest] = added;
      return this.change(serverKey, channelId, {
        nowPlaying: first,
        queue: [...rest, ...queue],
        state: MusicPlayState.Playing,
        positionMs: 0,
        anchorMs: this.now(),
      });
    } else if (beforeItemId) {
      const at = queue.findIndex((entry) => entry.itemId === beforeItemId);
      queue.splice(at < 0 ? queue.length : at, 0, ...added);
    } else queue = [...queue, ...added];
    return this.change(serverKey, channelId, { queue });
  }

  async musicPlayback(
    serverKey: string,
    channelId: string,
    action: MusicAction,
    positionMs: number | undefined,
    expectedItemId: string | undefined,
  ): Promise<MusicSessionRecord> {
    this.enter('musicPlayback', [serverKey, channelId, action, positionMs, expectedItemId]);
    const current = this.session(serverKey, channelId);
    if (expectedItemId !== undefined && current.nowPlaying?.itemId !== expectedItemId) return current;
    const now = this.now();
    const at = this.positionAt(current, now);
    switch (action) {
      case MusicAction.Pause:
        return this.change(serverKey, channelId, { state: MusicPlayState.Paused, positionMs: at, anchorMs: now });
      case MusicAction.Resume:
        return this.change(serverKey, channelId, { state: MusicPlayState.Playing, positionMs: at, anchorMs: now });
      case MusicAction.Seek:
        return this.change(serverKey, channelId, { positionMs: positionMs ?? 0, anchorMs: now });
      case MusicAction.Stop:
        return this.change(serverKey, channelId, { state: MusicPlayState.Idle, nowPlaying: undefined, queue: [], positionMs: 0 });
      case MusicAction.Next: {
        const [next, ...rest] = current.queue;
        const played: MusicQueueItemRecord[] = current.nowPlaying ? [current.nowPlaying, ...current.history] : current.history;
        return this.change(serverKey, channelId, {
          nowPlaying: next,
          queue: rest,
          history: played.slice(0, 50),
          state: next ? MusicPlayState.Playing : MusicPlayState.Idle,
          positionMs: 0,
          anchorMs: now,
          skipVotes: [],
        });
      }
      case MusicAction.Previous: {
        const [previous, ...older] = current.history;
        if (!previous) return this.change(serverKey, channelId, { positionMs: 0, anchorMs: now });
        const queue = current.nowPlaying ? [current.nowPlaying, ...current.queue] : current.queue;
        return this.change(serverKey, channelId, { nowPlaying: previous, history: older, queue, positionMs: 0, anchorMs: now });
      }
    }
  }

  async musicSetMode(
    serverKey: string,
    channelId: string,
    repeat: MusicRepeat | undefined,
    shuffle: boolean | undefined,
  ): Promise<MusicSessionRecord> {
    this.enter('musicSetMode', [serverKey, channelId, repeat, shuffle]);
    const current = this.session(serverKey, channelId);
    return this.change(serverKey, channelId, { repeat: repeat ?? current.repeat, shuffle: shuffle ?? current.shuffle });
  }

  async musicQueueMove(serverKey: string, channelId: string, itemId: string, beforeItemId: string | undefined): Promise<MusicSessionRecord> {
    this.enter('musicQueueMove', [serverKey, channelId, itemId, beforeItemId]);
    const queue = [...this.session(serverKey, channelId).queue];
    const from = queue.findIndex((entry) => entry.itemId === itemId);
    const [moved] = from < 0 ? [] : queue.splice(from, 1);
    if (moved) {
      const to = beforeItemId === undefined ? -1 : queue.findIndex((entry) => entry.itemId === beforeItemId);
      queue.splice(to < 0 ? queue.length : to, 0, moved);
    }
    return this.change(serverKey, channelId, { queue });
  }

  async musicQueueRemove(serverKey: string, channelId: string, itemIds: string[]): Promise<MusicSessionRecord> {
    this.enter('musicQueueRemove', [serverKey, channelId, itemIds]);
    const queue = this.session(serverKey, channelId).queue.filter((entry) => !itemIds.includes(entry.itemId));
    return this.change(serverKey, channelId, { queue });
  }

  async musicQueueClear(serverKey: string, channelId: string): Promise<MusicSessionRecord> {
    this.enter('musicQueueClear', [serverKey, channelId]);
    return this.change(serverKey, channelId, { queue: [] });
  }

  async musicVoteSkip(serverKey: string, channelId: string, expectedItemId: string): Promise<MusicSessionRecord> {
    this.enter('musicVoteSkip', [serverKey, channelId, expectedItemId]);
    const current = this.session(serverKey, channelId);
    if (current.nowPlaying?.itemId !== expectedItemId) return current;
    const skipVotes = [...new Set([...current.skipVotes, SELF_ID])];
    return this.change(serverKey, channelId, { skipVotes });
  }

  musicSetListening(volume: number, muted: boolean, duck: boolean): void {
    this.enter('musicSetListening', [volume, muted, duck]);
  }

  async musicArtwork(serverKey: string, artworkId: string, size: number): Promise<string> {
    this.enter('musicArtwork', [serverKey, artworkId, size]);
    return `/fake/artwork/${artworkId}-${size}.jpg`;
  }

  async musicUploadSong(serverKey: string, channelId: string, uploadId: string, path: string, playNow: boolean): Promise<void> {
    this.enter('musicUploadSong', [serverKey, channelId, uploadId, path, playNow]);
    this.requireConnected(serverKey);
    const filename = path.split('/').pop() ?? path;
    const upload: Upload = { serverKey, cancelled: false };
    this.uploads.set(uploadId, upload);
    const totalBytes = 4_000_000;
    const progress = (phase: UploadPhaseRecord, sentBytes: number) =>
      this.emit(new MobileEvent.MusicUpload({ serverKey, channelId, uploadId, filename, sentBytes, totalBytes, phase }));
    const step = () => new Promise<void>((resolve) => setTimeout(resolve, this.uploadStepMs));
    try {
      progress(new UploadPhaseRecord.Preparing(), 0);
      for (const sent of [0, totalBytes / 2, totalBytes]) {
        await step();
        if (upload.cancelled) {
          progress(new UploadPhaseRecord.Cancelled(), sent);
          throw new MobileError.Cancelled();
        }
        progress(new UploadPhaseRecord.Uploading(), sent);
      }
      await step();
      progress(new UploadPhaseRecord.Processing(), totalBytes);
      if (filename === 'fail.mp3') {
        const message = "That file isn't a song this server can play.";
        progress(new UploadPhaseRecord.Failed({ message }), totalBytes);
        throw new MobileError.Server({ code: 'invalid_argument', message, retryAfterMs: undefined });
      }
      const title = filename.replace(/\.[^.]+$/, '');
      const queued = item({ ...track(title, this.name, 180_000, true), title }, SELF_ID, this.now());
      const current = this.session(serverKey, channelId);
      this.change(serverKey, channelId, playNow ? { nowPlaying: queued, state: MusicPlayState.Playing } : { queue: [...current.queue, queued] });
      progress(new UploadPhaseRecord.Done(), totalBytes);
    } finally {
      this.uploads.delete(uploadId);
    }
  }

  musicCancelSong(serverKey: string, uploadId: string): void {
    this.enter('musicCancelSong', [serverKey, uploadId]);
    const upload = this.uploads.get(uploadId);
    if (upload) upload.cancelled = true;
  }

  private session(serverKey: string, channelId: string): MusicSessionRecord {
    const key = this.key(serverKey, channelId);
    const existing = this.sessions.get(key);
    if (existing) return existing;
    const fresh = { ...session(channelId, this.now()), state: MusicPlayState.Idle, nowPlaying: undefined, queue: [], history: [] };
    this.sessions.set(key, fresh);
    return fresh;
  }

  /** A new version of the session, sent as the core sends it. */
  private change(serverKey: string, channelId: string, change: Partial<MusicSessionRecord>): MusicSessionRecord {
    const current = this.session(serverKey, channelId);
    const next: MusicSessionRecord = { ...current, ...change, version: current.version + 1 };
    this.sessions.set(this.key(serverKey, channelId), next);
    this.emit(new MobileEvent.MusicSession({ serverKey, session: next }));
    return next;
  }

  private positionAt(current: MusicSessionRecord, now: number): number {
    const elapsed = current.state === MusicPlayState.Playing && now > current.anchorMs ? now - current.anchorMs : 0;
    const length = current.nowPlaying?.track.durationMs ?? 0;
    const at = current.positionMs + elapsed;
    return length > 0 && at > length ? length : at;
  }

  // Plumbing -----------------------------------------------------------

  private key(serverKey: string, channelId: string): string {
    return `${serverKey}/${channelId}`;
  }

  private id(): number {
    this.nextId += 1;
    return this.nextId;
  }

  private enter(method: keyof CoreApi, args: unknown[]): void {
    this.calls.push({ method, args });
    if (this.failures.has(method)) {
      const error = this.failures.get(method);
      this.failures.delete(method);
      throw error;
    }
  }
}
