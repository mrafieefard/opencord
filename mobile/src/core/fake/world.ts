// The fake world FakeCore serves (plan Task 4): fixed ids, times relative to
// `now`, and every state a screen has to show. Three servers: Night Owls,
// connected, with chat, voice and music; Rust Club, reconnecting; Weekend
// Raid, refused.
import {
  AttachmentKind,
  ChannelKind,
  MusicPlayState,
  MusicRepeat,
  type ChannelAccess,
  type ChannelRecord,
  type DefaultSoundRecord,
  type MemberRecord,
  type MessageRecord,
  type MusicQueueItemRecord,
  type MusicSessionRecord,
  type MusicSettingsRecord,
  type MusicTrackRecord,
  type ServerEntry,
  type ServerSnapshot,
  type SoundboardSoundRecord,
  type UserRecord,
  type VoiceStateRecord,
} from '@opencord/core-native';

export const SELF_ID = '1000';
export const NIGHT_OWLS = 'fake-night-owls';
export const RUST_CLUB = 'fake-rust-club';
export const WEEKEND_RAID = 'fake-weekend-raid';

export const CHANNELS = {
  general: '10',
  musicRequests: '11',
  textCategory: '30',
  offTopic: '12',
  voiceCategory: '31',
  lounge: '20',
  quiet: '21',
} as const;

const MINUTE = 60_000;

const people: [string, string][] = [
  [SELF_ID, 'Ava'],
  ['1001', 'Kai'],
  ['1002', 'Mira'],
  ['1003', 'Jonas'],
  ['1004', 'Priya'],
  ['1005', 'Tomás'],
  ['1006', 'Noor'],
  ['1007', 'Leo'],
];

export function user(id: string): UserRecord {
  const name = people.find(([personId]) => personId === id)?.[1] ?? `Person ${id}`;
  return { id, displayName: name, fingerprint: `FAKE-${id}-0000-0000` };
}

function members(): MemberRecord[] {
  return people.map(([id]) => ({ user: user(id), roleIds: [] }));
}

function channel(
  id: string,
  kind: ChannelKind,
  name: string,
  position: number,
  parentId?: string,
  topic?: string,
): ChannelRecord {
  return { id, kind, name, topic, parentId, position, userLimit: 0, textInVoice: false };
}

export function fullAccess(channelId: string): ChannelAccess {
  return {
    channelId,
    view: true,
    readHistory: true,
    send: true,
    attach: true,
    connect: true,
    speak: true,
    manageMessages: false,
    musicAdd: true,
    musicControl: true,
    musicManageQueue: true,
    musicManage: false,
    useSoundboard: true,
    useExternalSounds: true,
  };
}

function sound(id: string, name: string, emoji: string | undefined, byte: string, durationMs: number): SoundboardSoundRecord {
  return {
    id,
    name,
    emoji,
    volume: 100,
    sha256: byte.repeat(64),
    sizeBytes: 24_000,
    durationMs,
    uploaderId: '1001',
    createdAtMs: 1_761_000_000_000,
  };
}

/** Night Owls has three sounds, Rust Club one; the others none. */
export function soundboardSounds(serverKey: string): SoundboardSoundRecord[] {
  if (serverKey === NIGHT_OWLS) {
    return [
      sound('800001', 'Airhorn', '📯', 'a', 1_800),
      sound('800002', 'Owl hoot', '🦉', 'b', 1_200),
      sound('800003', 'Rimshot', undefined, 'c', 900),
    ];
  }
  if (serverKey === RUST_CLUB) return [sound('800101', 'Ferris', '🦀', 'd', 1_000)];
  return [];
}

/** The defaults every client has (opencord-common's DEFAULT_SOUNDS, in part). */
export const defaultSounds: DefaultSoundRecord[] = [
  { id: 'chime', name: 'Chime', emoji: '🔔', durationMs: 1_400 },
  { id: 'tada', name: 'Ta-da', emoji: '🎉', durationMs: 1_600 },
  { id: 'air-horn', name: 'Air horn', emoji: '📯', durationMs: 1_750 },
  { id: 'applause', name: 'Applause', emoji: '👏', durationMs: 3_000 },
];

function nightOwlsChannels(): ChannelRecord[] {
  return [
    channel(CHANNELS.general, ChannelKind.Text, 'general', 0, undefined, 'The usual chatter'),
    channel(CHANNELS.musicRequests, ChannelKind.Text, 'music-requests', 1),
    channel(CHANNELS.textCategory, ChannelKind.Category, 'Text', 2),
    channel(CHANNELS.offTopic, ChannelKind.Text, 'off-topic', 0, CHANNELS.textCategory),
    channel(CHANNELS.voiceCategory, ChannelKind.Category, 'Voice', 3),
    channel(CHANNELS.lounge, ChannelKind.Voice, 'Lounge', 0, CHANNELS.voiceCategory),
    channel(CHANNELS.quiet, ChannelKind.Voice, 'Quiet room', 1, CHANNELS.voiceCategory),
  ];
}

function voiceState(userId: string, channelId: string, selfMute = false): VoiceStateRecord {
  return {
    userId,
    channelId,
    thisDevice: false,
    selfMute,
    selfDeaf: false,
    serverMute: false,
    serverDeaf: false,
  };
}

export function snapshot(serverKey: string): ServerSnapshot {
  const channels = serverKey === NIGHT_OWLS ? nightOwlsChannels() : [];
  return {
    serverKey,
    selfUser: user(SELF_ID),
    name: entries().find((entry) => entry.key === serverKey)?.name ?? serverKey,
    description: '',
    ownerId: '1001',
    channels,
    access: channels.map((entry) => fullAccess(entry.id)),
    members: members(),
    voiceStates:
      serverKey === NIGHT_OWLS
        ? [voiceState('1001', CHANNELS.lounge), voiceState('1002', CHANNELS.lounge, true)]
        : [],
    voiceEnabled: true,
    soundboard: { enabled: true, allowDefaultSounds: true, allowExternalSounds: true, cooldownMs: 3_000 },
    soundboardSounds: soundboardSounds(serverKey),
  };
}

export function entries(): ServerEntry[] {
  return [
    { key: NIGHT_OWLS, name: 'Night Owls', host: 'night-owls.example', port: 7710, userId: SELF_ID },
    { key: RUST_CLUB, name: 'Rust Club', host: 'rust.example', port: 7710, userId: SELF_ID },
    { key: WEEKEND_RAID, name: 'Weekend Raid', host: 'raid.example', port: 7710 },
  ];
}

const lines: [string, string][] = [
  ['1001', 'evening all'],
  ['1002', 'hey **Kai**, did you see the new build?'],
  ['1001', 'yes! the queue finally keeps its order'],
  ['1003', '> the queue finally keeps its order\nabout time'],
  ['1004', 'anyone up for a round later?'],
  [SELF_ID, 'I am, after dinner'],
  ['1005', 'here is the config I use:\n```\nbitrate = 96000\nchannels = 2\n```'],
  ['1002', 'thanks <@1005>, trying it now'],
  ['1006', 'the release notes are at https://example.com/notes'],
  ['1001', 'I put a few songs in the queue in <#20>'],
  ['1007', '_quietly_ joins'],
  ['1003', 'did the ~~old~~ new mic arrive?'],
];

/** `count` messages in `channelId`, oldest first, ending a few minutes before `now`. */
export function history(channelId: string, count: number, now: number, firstId: number): MessageRecord[] {
  return Array.from({ length: count }, (_, index) => {
    const [authorId, content] = lines[index % lines.length] ?? ['1001', 'hi'];
    const message: MessageRecord = {
      id: String(firstId + index),
      channelId,
      authorId,
      content,
      createdAtMs: now - (count - index) * 7 * MINUTE,
      attachments: [],
    };
    if (index === count - 20) {
      message.content = '';
      message.attachments = [
        {
          id: String(firstId + index + 50_000),
          filename: 'sunset.jpg',
          contentType: 'image/jpeg',
          sizeBytes: 812_000,
          kind: AttachmentKind.Image,
          width: 1600,
          height: 1200,
          hasThumbnail: true,
          thumbnailWidth: 400,
          thumbnailHeight: 300,
          animated: false,
        },
      ];
    }
    if (index === count - 3) {
      message.content = 'Check out these photos and the project spec:';
      message.attachments = [
        {
          id: String(firstId + index + 50_000),
          filename: 'sunset.jpg',
          contentType: 'image/jpeg',
          sizeBytes: 812_000,
          kind: AttachmentKind.Image,
          width: 1600,
          height: 1200,
          hasThumbnail: true,
          thumbnailWidth: 400,
          thumbnailHeight: 300,
          animated: false,
        },
        {
          id: String(firstId + index + 50_001),
          filename: 'aurora.png',
          contentType: 'image/png',
          sizeBytes: 1_240_000,
          kind: AttachmentKind.Image,
          width: 1200,
          height: 800,
          hasThumbnail: true,
          thumbnailWidth: 400,
          thumbnailHeight: 266,
          animated: false,
        },
        {
          id: String(firstId + index + 50_002),
          filename: 'client-spec.pdf',
          contentType: 'application/pdf',
          sizeBytes: 2_450_000,
          kind: AttachmentKind.File,
          hasThumbnail: false,
          animated: false,
        },
      ];
    }
    return message;
  });
}

export function track(title: string, artist: string, durationMs: number, local = false): MusicTrackRecord {
  return {
    title,
    artist,
    durationMs,
    isStream: false,
    isSeekable: true,
    source: local ? 'local' : 'youtube',
    sourceDisplayName: local ? 'Local' : 'YouTube',
    uri: local ? '' : `https://music.example/${encodeURIComponent(title)}`,
    artworkId: local ? '' : `art-${title}`,
    localSongId: local ? `7${title.length}` : undefined,
    removable: local,
  };
}

export const catalogue: MusicTrackRecord[] = [
  track('Midnight City', 'M83', 243_000),
  track('Night Owl', 'Galimatias', 210_000),
  track('Breathe', 'Télépopmusik', 280_000),
  track('Holocene', 'Bon Iver', 337_000),
  track('Our dinner theme', 'Ava', 95_000, true),
  track('Weightless', 'Marconi Union', 480_000),
  track('Intro', 'The xx', 128_000),
  track('Teardrop', 'Massive Attack', 330_000),
];

let nextItem = 1;
export function item(trackRecord: MusicTrackRecord, requesterId: string, addedAtMs: number): MusicQueueItemRecord {
  nextItem += 1;
  return { itemId: `item-${nextItem}`, track: trackRecord, requesterId, addedAtMs };
}

export function session(channelId: string, now: number): MusicSessionRecord {
  const playing = channelId === CHANNELS.lounge;
  const [first, ...rest] = catalogue;
  return {
    channelId,
    version: 1,
    state: playing ? MusicPlayState.Playing : MusicPlayState.Idle,
    nowPlaying: playing && first ? item(first, '1001', now - 2 * MINUTE) : undefined,
    queue: playing ? rest.slice(0, 6).map((entry) => item(entry, '1002', now - MINUTE)) : [],
    history: playing
      ? [...catalogue, ...catalogue].slice(0, 10).map((entry) => item(entry, '1003', now - 30 * MINUTE))
      : [],
    positionMs: playing ? 72_000 : 0,
    anchorMs: now,
    repeat: MusicRepeat.Off,
    shuffle: false,
    waitingForNode: false,
    pausedEmpty: false,
    skipVotes: [],
    skipVotesNeeded: 2,
  };
}

export function musicSettings(): MusicSettingsRecord {
  return {
    supported: true,
    enabled: true,
    configured: true,
    hasNodes: true,
    localSongs: true,
    availableSources: ['youtube', 'soundcloud'],
    defaultSource: 'youtube',
    allowLinks: true,
    voteSkip: true,
  };
}
