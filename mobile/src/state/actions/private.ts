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
    const mergedContacts = { ...state.private.contacts };
    for (const contact of contacts) {
      if (!state.private.touchedPeers.includes(contact.peer)) {
        mergedContacts[contact.peer] = contact;
      }
    }
    return {
      ...state,
      private: {
        ...state.private,
        loaded: true,
        contacts: mergedContacts,
        ownDevices,
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

export async function loadMessages(peer: string): Promise<void> {
  const messages = await getCore().privateMessages(peer, undefined, 50);
  useAppStore.setState((state) => {
    const sorted = [...messages].sort(comparePrivateMessages);
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
  await getCore().privateEdit(peer, messageId, content);
  useAppStore.setState((state) => {
    const list = state.private.messages[peer];
    if (!list) return state;
    return {
      ...state,
      private: {
        ...state.private,
        messages: {
          ...state.private.messages,
          [peer]: list.map((m) => (m.id === messageId ? { ...m, content, editedAtMs: Date.now() } : m)),
        },
      },
    };
  });
}

export async function deletePrivateMessage(peer: string, messageId: string): Promise<void> {
  await getCore().privateDelete(peer, messageId);
  useAppStore.setState((state) => {
    const list = state.private.messages[peer];
    if (!list) return state;
    return {
      ...state,
      private: {
        ...state.private,
        messages: {
          ...state.private.messages,
          [peer]: list.filter((m) => m.id !== messageId),
        },
      },
    };
  });
}

export async function retryPrivateMessage(messageId: string): Promise<void> {
  return getCore().privateRetry(messageId);
}

export async function sendFriendRequest(serverKey: string, userId: string): Promise<void> {
  return getCore().friendRequestSend(serverKey, userId);
}

export async function answerFriendRequest(peer: string, accept: boolean): Promise<void> {
  await getCore().friendRequestAnswer(peer, accept);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, friend: accept ? FriendStatus.Friends : FriendStatus.None },
        },
      },
    };
  });
}

export async function cancelFriendRequest(peer: string): Promise<void> {
  await getCore().friendRequestCancel(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, friend: FriendStatus.None },
        },
      },
    };
  });
}

export async function removeFriend(peer: string): Promise<void> {
  await getCore().friendRemove(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, friend: FriendStatus.None },
        },
      },
    };
  });
}

export async function acceptMessageRequest(peer: string): Promise<void> {
  await getCore().messageRequestAccept(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, conversation: ConversationStatus.Accepted },
        },
      },
    };
  });
}

export async function ignoreMessageRequest(peer: string): Promise<void> {
  await getCore().messageRequestIgnore(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, conversation: ConversationStatus.None },
        },
      },
    };
  });
}

export async function blockPeer(peer: string): Promise<void> {
  await getCore().privateBlock(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, blocked: true, friend: FriendStatus.None },
        },
      },
    };
  });
}

export async function unblockPeer(peer: string): Promise<void> {
  await getCore().privateUnblock(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, blocked: false },
        },
      },
    };
  });
}

export async function setPrivacySettings(settings: PrivacySettingsRecord): Promise<void> {
  return getCore().privacySettingsSet(settings);
}

export async function openPrivateConversation(serverKey: string, userId: string): Promise<PrivateContactRecord> {
  const contact = await getCore().privateOpenConversation(serverKey, userId);
  useAppStore.setState((state) => ({
    ...state,
    private: {
      ...state.private,
      contacts: {
        ...state.private.contacts,
        [contact.peer]: contact,
      },
    },
  }));
  return contact;
}

export async function movePrivate(peer: string, serverKey: string): Promise<void> {
  await getCore().privateMove(peer, serverKey);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, relayServerKey: serverKey },
        },
      },
    };
  });
}

export async function movePrivateToo(peer: string, messageId: string): Promise<void> {
  return getCore().privateMoveToo(peer, messageId);
}

export async function sendPrivateTyping(peer: string): Promise<void> {
  return getCore().privateTyping(peer);
}

export async function markPrivateRead(peer: string): Promise<void> {
  await getCore().privateMarkRead(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact || contact.unread === 0) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, unread: 0 },
        },
      },
    };
  });
}

export async function setPrivateMuted(peer: string, muted: boolean): Promise<void> {
  await getCore().privateSetMuted(peer, muted);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, muted },
        },
      },
    };
  });
}

export async function hidePrivate(peer: string): Promise<void> {
  await getCore().privateHide(peer);
  useAppStore.setState((state) => {
    const contact = state.private.contacts[peer];
    if (!contact) return state;
    return {
      ...state,
      private: {
        ...state.private,
        contacts: {
          ...state.private.contacts,
          [peer]: { ...contact, hidden: true },
        },
      },
    };
  });
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
