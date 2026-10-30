// The Rust core as the app sees it (design §5): one method per facade
// function, with the same names and types. NativeCore is the real one;
// FakeCore answers the same way for tests and screenshots.
import type {
  AddServerResult,
  AttachmentVariant,
  DefaultSoundRecord,
  ExternalSoundRecord,
  IdentityInfo,
  MessageRecord,
  MobileEvent,
  MusicAction,
  MusicPlacement,
  MusicRepeat,
  MusicSearchResultsRecord,
  MusicSessionRecord,
  MusicSettingsRecord,
  NewIdentity,
  OutgoingFileRecord,
  ServerEntry,
  UserRecord,
  VoiceStateRecord,
} from '@opencord/core-native';

export type * from '@opencord/core-native';

export interface CoreApi {
  coreInit(dataDir: string, cacheDir: string): void;
  coreVersion(): string;
  eventsAttach(): bigint;
  nextEvents(generation: bigint, max: number): Promise<MobileEvent[]>;
  settingsGet(key: string): string | undefined;
  settingsSet(key: string, value: string | undefined): void;

  identityGenerate(): NewIdentity;
  identityCheck(secret: ArrayBuffer, displayName: string): IdentityInfo;
  identityLoad(secret: ArrayBuffer, displayName: string): IdentityInfo;
  identityBackupEncode(secret: ArrayBuffer): string;
  identityBackupDecode(text: string): ArrayBuffer;

  serversList(): ServerEntry[];
  serverAdd(linkOrAddress: string, claimToken: string | undefined): Promise<AddServerResult>;
  serverTrust(address: string, fingerprint: string): void;
  serverRetryNow(serverKey: string): Promise<void>;
  serverRemove(serverKey: string): Promise<void>;
  profileUpdate(serverKey: string, displayName: string): Promise<UserRecord>;
  avatarFile(serverKey: string, hash: string): Promise<string>;

  messagesFetch(serverKey: string, channelId: string, before: string | undefined, limit: number): Promise<MessageRecord[]>;
  messageSend(serverKey: string, channelId: string, content: string, nonce: string): Promise<MessageRecord>;
  messageSendFiles(
    serverKey: string,
    channelId: string,
    content: string,
    nonce: string,
    files: OutgoingFileRecord[],
  ): Promise<MessageRecord>;
  sendCancel(serverKey: string, nonce: string): void;
  messageEdit(serverKey: string, messageId: string, content: string): Promise<MessageRecord>;
  messageDelete(serverKey: string, messageId: string): Promise<void>;
  typingStart(serverKey: string, channelId: string): Promise<void>;
  attachmentFile(serverKey: string, attachmentId: string, variant: AttachmentVariant): Promise<string>;

  voiceJoin(serverKey: string, channelId: string): Promise<VoiceStateRecord>;
  voiceLeave(): Promise<void>;
  voiceSetMute(muted: boolean): void;
  voiceSetDeaf(deafened: boolean): void;
  voiceSetUserVolume(serverKey: string, userId: string, volume: number): void;
  voiceSetUserLocalMute(serverKey: string, userId: string, muted: boolean): void;

  musicSettings(serverKey: string): Promise<MusicSettingsRecord>;
  musicSubscribe(serverKey: string, channelId: string): Promise<MusicSessionRecord>;
  musicUnsubscribe(serverKey: string, channelId: string): Promise<void>;
  musicSearch(serverKey: string, channelId: string, query: string, source: string): Promise<MusicSearchResultsRecord>;
  musicEnqueue(
    serverKey: string,
    channelId: string,
    resultIds: string[],
    historyItemIds: string[],
    beforeItemId: string | undefined,
    placement: MusicPlacement,
  ): Promise<MusicSessionRecord>;
  musicPlayback(
    serverKey: string,
    channelId: string,
    action: MusicAction,
    positionMs: number | undefined,
    expectedItemId: string | undefined,
  ): Promise<MusicSessionRecord>;
  musicSetMode(
    serverKey: string,
    channelId: string,
    repeat: MusicRepeat | undefined,
    shuffle: boolean | undefined,
  ): Promise<MusicSessionRecord>;
  musicQueueMove(serverKey: string, channelId: string, itemId: string, beforeItemId: string | undefined): Promise<MusicSessionRecord>;
  musicQueueRemove(serverKey: string, channelId: string, itemIds: string[]): Promise<MusicSessionRecord>;
  musicQueueClear(serverKey: string, channelId: string): Promise<MusicSessionRecord>;
  musicVoteSkip(serverKey: string, channelId: string, expectedItemId: string): Promise<MusicSessionRecord>;
  musicSetListening(volume: number, muted: boolean, duck: boolean): void;
  musicArtwork(serverKey: string, artworkId: string, size: number): Promise<string>;
  musicUploadSong(serverKey: string, channelId: string, uploadId: string, path: string, playNow: boolean): Promise<void>;
  musicCancelSong(serverKey: string, uploadId: string): void;

  soundboardDefaultSounds(): DefaultSoundRecord[];
  soundboardPlay(serverKey: string, channelId: string, soundRef: string, external: ExternalSoundRecord | undefined): Promise<void>;
  soundboardPreview(serverKey: string, soundRef: string): Promise<void>;
}
