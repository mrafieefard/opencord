import {
  ConversationStatus,
  FriendStatus,
  type PrivacySettingsRecord,
  type PrivateContactRecord,
  type PrivateMessageRecord,
} from '@opencord/core-native';

import { getCore } from '@/core';
import { comparePrivateMessages } from '../private';
import { useAppStore } from '../store';

export async function loadPrivate(): Promise<void> {
  const core = getCore();
  const [contacts, ownDevices] = await Promise.all([
    core.privateContacts(),
    core.privateOwnDevices(),
  ]);

  useAppStore.setState((state) => {
    const corePeers = new Set(contacts.map((c) => c.peer));
    const mergedContacts: Record<string, PrivateContactRecord> = {};
    for (const contact of contacts) {
      if (state.private.touchedPeers.includes(contact.peer) && state.private.contacts[contact.peer]) {
        mergedContacts[contact.peer] = state.private.contacts[contact.peer]!;
      } else if (!state.private.touchedPeers.includes(contact.peer)) {
        mergedContacts[contact.peer] = contact;
      }
    }
    for (const peer of state.private.touchedPeers) {
      if (!corePeers.has(peer) && state.private.contacts[peer]) {
        mergedContacts[peer] = state.private.contacts[peer]!;
      }
    }
    return {
      ...state,
      private: {
        ...state.private,
        loaded: true,
        contacts: mergedContacts,
        ownDevices,
        touchedPeers: [],
      },
    };
  });
}

export async function loadOlder(peer: string): Promise<void> {
  const current = useAppStore.getState().private;
  if (current.loadingOlder[peer] || current.hasMore[peer] === false) return;

  useAppStore.setState((state) => ({
    ...state,
    private: {
      ...state.private,
      loadingOlder: { ...state.private.loadingOlder, [peer]: true },
    },
  }));

  try {
    const list = current.messages[peer] ?? [];
    const oldestId = list[0]?.id;
    const older = await getCore().privateMessages(peer, oldestId, 50);

    useAppStore.setState((state) => {
      const existing = state.private.messages[peer] ?? [];
      const existingIds = new Set(existing.map((m) => m.id));
      const fresh = older.filter((m) => !existingIds.has(m.id));
      const merged = [...fresh, ...existing].sort(comparePrivateMessages);

      return {
        ...state,
        private: {
          ...state.private,
          messages: { ...state.private.messages, [peer]: merged },
          hasMore: { ...state.private.hasMore, [peer]: older.length >= 50 },
          loadingOlder: { ...state.private.loadingOlder, [peer]: false },
        },
      };
    });
  } catch (error) {
    useAppStore.setState((state) => ({
      ...state,
      private: {
        ...state.private,
        loadingOlder: { ...state.private.loadingOlder, [peer]: false },
      },
    }));
    throw error;
  }
}

/**
 * The newest page of a chat. Merged with what the store holds, never in
 * place of it: a message the event stream added or changed while the page
 * loaded is newer than the page's copy, so the store's copy wins.
 */
export async function loadMessages(peer: string): Promise<void> {
  const messages = await getCore().privateMessages(peer, undefined, 50);
  useAppStore.setState((state) => {
    const held = state.private.messages[peer] ?? [];
    const heldIds = new Set(held.map((m) => m.id));
    const sorted = [...held, ...messages.filter((m) => !heldIds.has(m.id))].sort(comparePrivateMessages);
    return {
      ...state,
      private: {
        ...state.private,
        messages: { ...state.private.messages, [peer]: sorted },
        hasMore: { ...state.private.hasMore, [peer]: messages.length >= 50 },
      },
    };
  });
}

export async function sendPrivateMessage(peer: string, content: string): Promise<PrivateMessageRecord> {
  return getCore().privateSend(peer, content);
}

export async function editPrivateMessage(peer: string, messageId: string, content: string): Promise<void> {
  return getCore().privateEdit(peer, messageId, content);
}

export async function deletePrivateMessage(peer: string, messageId: string): Promise<void> {
  return getCore().privateDelete(peer, messageId);
}

export async function retryPrivateMessage(messageId: string): Promise<void> {
  return getCore().privateRetry(messageId);
}

export async function sendFriendRequest(serverKey: string, userId: string): Promise<void> {
  return getCore().friendRequestSend(serverKey, userId);
}

export async function answerFriendRequest(peer: string, accept: boolean): Promise<void> {
  return getCore().friendRequestAnswer(peer, accept);
}

export async function cancelFriendRequest(peer: string): Promise<void> {
  return getCore().friendRequestCancel(peer);
}

export async function removeFriend(peer: string): Promise<void> {
  return getCore().friendRemove(peer);
}

export async function acceptMessageRequest(peer: string): Promise<void> {
  return getCore().messageRequestAccept(peer);
}

export async function ignoreMessageRequest(peer: string): Promise<void> {
  return getCore().messageRequestIgnore(peer);
}

export async function blockPeer(peer: string): Promise<void> {
  return getCore().privateBlock(peer);
}

export async function unblockPeer(peer: string): Promise<void> {
  return getCore().privateUnblock(peer);
}

export async function setPrivacySettings(settings: PrivacySettingsRecord): Promise<void> {
  return getCore().privacySettingsSet(settings);
}

export async function openPrivateConversation(serverKey: string, userId: string): Promise<PrivateContactRecord> {
  return getCore().privateOpenConversation(serverKey, userId);
}

export async function movePrivate(peer: string, serverKey: string): Promise<void> {
  return getCore().privateMove(peer, serverKey);
}

export async function movePrivateToo(peer: string, messageId: string): Promise<void> {
  return getCore().privateMoveToo(peer, messageId);
}

export async function sendPrivateTyping(peer: string): Promise<void> {
  return getCore().privateTyping(peer);
}

export async function markPrivateRead(peer: string): Promise<void> {
  return getCore().privateMarkRead(peer);
}

export async function setPrivateMuted(peer: string, muted: boolean): Promise<void> {
  return getCore().privateSetMuted(peer, muted);
}

export async function hidePrivate(peer: string): Promise<void> {
  return getCore().privateHide(peer);
}

export async function approveDevice(deviceId: string, keys: string): Promise<void> {
  return getCore().privateApproveDevice(deviceId, keys);
}

export async function acceptKeysFrom(deviceId: string, matched: boolean, keys: string): Promise<void> {
  return getCore().privateAcceptKeysFrom(deviceId, matched, keys);
}

export async function verifyPeer(peer: string, newKeys: boolean, matched: boolean, keys: string): Promise<void> {
  return getCore().privateVerify(peer, newKeys, matched, keys);
}

export async function acceptNewKeys(peer: string): Promise<void> {
  return getCore().privateAcceptNewKeys(peer);
}

export async function rejectDevice(deviceId: string): Promise<void> {
  return getCore().privateRejectDevice(deviceId);
}

// Aliases matching core function names
export {
  approveDevice as privateApproveDevice,
  acceptKeysFrom as privateAcceptKeysFrom,
  acceptNewKeys as privateAcceptNewKeys,
  rejectDevice as privateRejectDevice,
  verifyPeer as privateVerify,
  hidePrivate as privateHide,
  setPrivateMuted as privateSetMuted,
  markPrivateRead as privateMarkRead,
  blockPeer as privateBlock,
  unblockPeer as privateUnblock,
  removeFriend as friendRemove,
  acceptMessageRequest as messageRequestAccept,
  ignoreMessageRequest as messageRequestIgnore,
  sendFriendRequest as friendRequestSend,
  answerFriendRequest as friendRequestAnswer,
  cancelFriendRequest as friendRequestCancel,
  sendPrivateMessage as privateSend,
  editPrivateMessage as privateEdit,
  deletePrivateMessage as privateDelete,
  retryPrivateMessage as privateRetry,
  sendPrivateTyping as privateTyping,
  openPrivateConversation as privateOpenConversation,
  movePrivate as privateMove,
  movePrivateToo as privateMoveToo,
};
