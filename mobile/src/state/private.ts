import {
  FriendStatus,
  OwnDevicesRecord,
  PrivateContactRecord,
  PrivateEventRecord,
  PrivateEventRecord_Tags,
  PrivateMessageKind,
  PrivateMessageRecord,
} from '@opencord/core-native';

import type { AppState } from './types';

export interface PrivateState {
  loaded: boolean;
  contacts: Record<string, PrivateContactRecord>;
  messages: Record<string, PrivateMessageRecord[]>;
  typing: Record<string, number>;
  ownDevices: OwnDevicesRecord;
  hasMore: Record<string, boolean>;
  loadingOlder: Record<string, boolean>;
  touchedPeers: string[];
}

export function initialPrivateState(): PrivateState {
  return {
    loaded: false,
    contacts: {},
    messages: {},
    typing: {},
    ownDevices: { devices: [], hasChats: false },
    hasMore: {},
    loadingOlder: {},
    touchedPeers: [],
  };
}

export function comparePrivateMessages(a: PrivateMessageRecord, b: PrivateMessageRecord): number {
  if (a.sentAtMs !== b.sentAtMs) {
    return a.sentAtMs - b.sentAtMs;
  }
  return a.id.localeCompare(b.id);
}

export function applyPrivateEvent(state: AppState, event: PrivateEventRecord, nowMs: number): AppState {
  switch (event.tag) {
    case PrivateEventRecord_Tags.OwnDevicesChanged: {
      return {
        ...state,
        private: {
          ...state.private,
          ownDevices: { ...event.inner },
        },
      };
    }
    case PrivateEventRecord_Tags.ContactChanged: {
      const contact = event.inner.contact;
      const touchedPeers = state.private.touchedPeers.includes(contact.peer)
        ? state.private.touchedPeers
        : [...state.private.touchedPeers, contact.peer];
      return {
        ...state,
        private: {
          ...state.private,
          contacts: { ...state.private.contacts, [contact.peer]: contact },
          touchedPeers,
        },
      };
    }
    case PrivateEventRecord_Tags.ContactRemoved: {
      const peer = event.inner.peer;
      const touchedPeers = state.private.touchedPeers.includes(peer)
        ? state.private.touchedPeers
        : [...state.private.touchedPeers, peer];
      const { [peer]: _removed, ...contacts } = state.private.contacts;
      return {
        ...state,
        private: {
          ...state.private,
          contacts,
          touchedPeers,
        },
      };
    }
    case PrivateEventRecord_Tags.MessageAdded: {
      const message = event.inner.message;
      const peer = message.peer;
      const existing = state.private.messages[peer] ?? [];
      const idx = existing.findIndex((m) => m.id === message.id);
      let nextMessages: PrivateMessageRecord[];
      if (idx !== -1) {
        nextMessages = [...existing];
        nextMessages[idx] = message;
      } else {
        nextMessages = [...existing, message].sort(comparePrivateMessages);
      }

      let nextContacts = state.private.contacts;
      const contact = nextContacts[peer];
      if (contact) {
        const preview = message.text
          ? message.text.split('\n')[0].slice(0, 100)
          : (message.kind === PrivateMessageKind.File ? 'Attachment' : contact.preview);
        const unread = !message.outgoing ? (contact.unread ?? 0) + 1 : contact.unread;
        const updatedContact: PrivateContactRecord = {
          ...contact,
          hidden: false,
          preview,
          unread,
          lastActivityMs: Math.max(contact.lastActivityMs ?? 0, message.sentAtMs),
        };
        nextContacts = { ...nextContacts, [peer]: updatedContact };
      }

      const { [peer]: _typingEnded, ...nextTyping } = state.private.typing;
      return {
        ...state,
        private: {
          ...state.private,
          messages: { ...state.private.messages, [peer]: nextMessages },
          contacts: nextContacts,
          typing: nextTyping,
        },
      };
    }
    case PrivateEventRecord_Tags.MessageChanged: {
      const message = event.inner.message;
      const peer = message.peer;
      const existing = state.private.messages[peer] ?? [];
      const idx = existing.findIndex((m) => m.id === message.id);
      if (idx !== -1) {
        const nextMessages = [...existing];
        nextMessages[idx] = message;
        return {
          ...state,
          private: {
            ...state.private,
            messages: { ...state.private.messages, [peer]: nextMessages },
          },
        };
      }
      return state;
    }
    case PrivateEventRecord_Tags.MessageDeleted: {
      const { peer, messageId } = event.inner;
      const existing = state.private.messages[peer] ?? [];
      const nextMessages = existing.filter((m) => m.id !== messageId);
      return {
        ...state,
        private: {
          ...state.private,
          messages: { ...state.private.messages, [peer]: nextMessages },
        },
      };
    }
    case PrivateEventRecord_Tags.Typing: {
      const peer = event.inner.peer;
      return {
        ...state,
        private: {
          ...state.private,
          typing: { ...state.private.typing, [peer]: nowMs + 10_000 },
        },
      };
    }
    case PrivateEventRecord_Tags.FriendRequestReceived: {
      const { peer, spam } = event.inner;
      const contact = state.private.contacts[peer];
      if (contact) {
        const updated: PrivateContactRecord = {
          ...contact,
          friend: FriendStatus.Incoming,
          requestSpam: spam,
        };
        return {
          ...state,
          private: {
            ...state.private,
            contacts: { ...state.private.contacts, [peer]: updated },
          },
        };
      }
      return state;
    }
    default:
      return state;
  }
}
