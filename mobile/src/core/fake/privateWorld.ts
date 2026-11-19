import {
  ConversationStatus,
  FriendStatus,
  KeyWordsRecord,
  OwnDeviceRecord,
  OwnDevicesRecord,
  OwnDeviceState,
  PrivacySettingsRecord,
  PrivateContactRecord,
  PrivateMessageKind,
  PrivateMessageRecord,
  PrivateSendState,
  PrivateVerification,
  SharedServerRecord,
} from '@opencord/core-native';

export const KAI_PEER = 'kai-peer';
export const MIRA_PEER = 'mira-peer';
export const ROWAN_PEER = 'rowan-peer';
export const SOL_PEER = 'sol-peer';
export const SPAM_PEER = 'spam-peer';
export const VEX_PEER = 'vex-peer';
export const ASH_PEER = 'ash-peer';

const WORDS = [
  'anchor', 'barrel', 'candle', 'dolphin', 'ember', 'falcon', 'garden',
  'harbor', 'island', 'jungle', 'kettle', 'lantern', 'meadow', 'nectar',
  'orchid', 'pepper', 'quartz', 'ribbon', 'saddle', 'timber', 'umbrella',
  'velvet', 'walnut', 'yellow',
];

export function wordsForSeed(seed: string): KeyWordsRecord {
  const words: string[] = [];
  for (let i = 0; i < 16; i += 1) {
    const code = seed.charCodeAt(i % seed.length);
    words.push(WORDS[(code + i * 7) % WORDS.length] ?? 'word');
  }
  const keys = Array.from(seed)
    .map((c) => c.charCodeAt(0).toString(16).padStart(2, '0'))
    .join('');
  return { words, keys };
}

export function createFakeContacts(now: number): Map<string, PrivateContactRecord> {
  const contacts = new Map<string, PrivateContactRecord>();

  // 1. Kai: online, verified, friend, active conversation
  contacts.set(KAI_PEER, {
    peer: KAI_PEER,
    name: 'Kai',
    friend: FriendStatus.Friends,
    requestSpam: false,
    blocked: false,
    conversation: ConversationStatus.Accepted,
    relayServerKey: 'fake-night-owls',
    lastActivityMs: now - 60_000,
    preview: 'Failed outgoing message waiting to be retried.',
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Verified,
    reachable: true,
  });

  // 2. Mira: friend, not verified, new keys
  contacts.set(MIRA_PEER, {
    peer: MIRA_PEER,
    name: 'Mira',
    friend: FriendStatus.Friends,
    requestSpam: false,
    blocked: false,
    conversation: ConversationStatus.Accepted,
    relayServerKey: 'fake-night-owls',
    lastActivityMs: now - 3_600_000,
    preview: 'Sounds good!',
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: true,
    verification: PrivateVerification.Unverified,
    reachable: true,
  });

  // 3. Rowan: incoming friend request (not spam)
  contacts.set(ROWAN_PEER, {
    peer: ROWAN_PEER,
    name: 'Rowan',
    friend: FriendStatus.Incoming,
    requestSpam: false,
    blocked: false,
    conversation: ConversationStatus.None,
    lastActivityMs: now - 7_200_000,
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Unverified,
    reachable: false,
  });

  // 4. Sol: outgoing friend request
  contacts.set(SOL_PEER, {
    peer: SOL_PEER,
    name: 'Sol',
    friend: FriendStatus.Outgoing,
    requestSpam: false,
    blocked: false,
    conversation: ConversationStatus.None,
    lastActivityMs: now - 14_400_000,
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Unverified,
    reachable: false,
  });

  // 5. Spam request
  contacts.set(SPAM_PEER, {
    peer: SPAM_PEER,
    name: 'Spam Sender',
    friend: FriendStatus.Incoming,
    requestSpam: true,
    blocked: false,
    conversation: ConversationStatus.None,
    lastActivityMs: now - 86_400_000,
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Unverified,
    reachable: false,
  });

  // 6. Vex: blocked
  contacts.set(VEX_PEER, {
    peer: VEX_PEER,
    name: 'Vex',
    friend: FriendStatus.None,
    requestSpam: false,
    blocked: true,
    conversation: ConversationStatus.None,
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Unverified,
    reachable: false,
  });

  // 7. Ash: message request
  contacts.set(ASH_PEER, {
    peer: ASH_PEER,
    name: 'Ash',
    friend: FriendStatus.None,
    requestSpam: false,
    blocked: false,
    conversation: ConversationStatus.Request,
    relayServerKey: 'fake-night-owls',
    lastActivityMs: now - 10_800_000,
    preview: 'Hi! Can I ask a question about the server rules?',
    unread: 1,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Unverified,
    reachable: true,
  });

  return contacts;
}

export function createFakePrivateMessages(now: number): Map<string, PrivateMessageRecord[]> {
  const map = new Map<string, PrivateMessageRecord[]>();

  // Kai's 60-message chat: oldest first
  const kaiMessages: PrivateMessageRecord[] = [];
  const startMs = now - 60 * 60_000;

  for (let i = 0; i < 60; i += 1) {
    const id = `000000000000${(1000 + i).toString(16)}`;
    const sentAtMs = startMs + i * 60_000;
    const outgoing = i % 2 === 1;

    let content = `Message ${i + 1} between you and Kai.`;
    let editedAtMs: number | undefined;
    let state = PrivateSendState.Sent;

    if (i === 30) {
      content = 'This message was edited by Kai.';
      editedAtMs = sentAtMs + 10_000;
    } else if (i === 40) {
      content = 'This message was deleted.';
    } else if (i === 59) {
      content = 'Failed outgoing message waiting to be retried.';
      state = PrivateSendState.Failed;
    }

    kaiMessages.push({
      id,
      peer: KAI_PEER,
      outgoing,
      kind: PrivateMessageKind.Text,
      content,
      serverKey: 'fake-night-owls',
      sentAtMs,
      editedAtMs,
      state,
    });
  }

  // Reverse so newest first in the store
  map.set(KAI_PEER, kaiMessages.reverse());

  // Ash's message request chat: 1 incoming message
  map.set(ASH_PEER, [
    {
      id: '0000000000002000',
      peer: ASH_PEER,
      outgoing: false,
      kind: PrivateMessageKind.Text,
      content: 'Hello! I saw your post on Night Owls.',
      serverKey: 'fake-night-owls',
      sentAtMs: now - 10_800_000,
      state: PrivateSendState.Sent,
    },
  ]);

  return map;
}

export function createFakeOwnDevices(): OwnDevicesRecord {
  const devices: OwnDeviceRecord[] = [
    {
      deviceId: 'a1b2c3d4e5f60718293a4b5c6d7e8f90',
      state: OwnDeviceState.Offering,
    },
  ];
  return {
    devices,
    hasChats: true,
  };
}

export function createFakePrivacySettings(): PrivacySettingsRecord {
  return {
    allowFriendRequests: true,
    allowMessageRequests: true,
  };
}

export function createFakeSharedServers(): SharedServerRecord[] {
  return [
    {
      serverKey: 'fake-night-owls',
      name: 'Night Owls',
      connected: true,
      privateMessages: true,
      friendsOnly: false,
      peerUserId: '1001',
    },
  ];
}
