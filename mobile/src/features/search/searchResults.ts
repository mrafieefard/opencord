import { ChannelKind } from '@opencord/core-native';

import type { AppState } from '@/state/types';

export interface SwitcherChannelItem {
  kind: 'channel';
  serverKey: string;
  serverName: string;
  channelId: string;
  name: string;
  channelKind?: ChannelKind;
}

export interface SwitcherServerItem {
  kind: 'server';
  serverKey: string;
  name: string;
  iconHash?: string;
}

export interface SwitcherMemberItem {
  kind: 'member';
  serverKey: string;
  userId: string;
  name: string;
  nickname?: string;
  avatarHash?: string;
}

export type SwitcherItem = SwitcherChannelItem | SwitcherServerItem | SwitcherMemberItem;

export const SWITCHER_LIMIT = 50;

const WORDS = /[\s\-_]+/;

/**
 * 0 for a prefix match, 1 for a word that starts with query, 2 when it
 * is anywhere in the name; null otherwise.
 */
export function scoreItem(name: string | undefined, query: string): number | null {
  if (!name) return null;
  const lower = name.toLowerCase();
  if (lower.startsWith(query)) return 0;
  if (lower.split(WORDS).some((word) => word.startsWith(query))) return 1;
  if (lower.includes(query)) return 2;
  return null;
}

export interface RecentChannelVisit {
  server: string;
  channel: string;
}

export function collectRecentChannels(state: {
  messages: AppState['messages'];
}): RecentChannelVisit[] {
  const visits: RecentChannelVisit[] = [];
  for (const key of Object.keys(state.messages)) {
    const [server, channel] = key.split('/');
    if (server && channel) {
      visits.push({ server, channel });
    }
  }
  return visits;
}

/**
 * Ranks items for query (§4.9): prefix matches before word matches before names
 * that merely contain it. `#`, `@` and `*` in front narrow to channels, members
 * and servers. With no query, recent channels come first.
 */
export function rankSwitcher(
  items: SwitcherItem[],
  query: string,
  recent: RecentChannelVisit[] = [],
): SwitcherItem[] {
  let needle = query.trim().toLowerCase();
  let filterKind: (item: SwitcherItem) => boolean = () => true;

  if (needle.length > 0) {
    switch (needle[0]) {
      case '#':
        filterKind = (item) => item.kind === 'channel';
        needle = needle.slice(1);
        break;
      case '@':
        filterKind = (item) => item.kind === 'member';
        needle = needle.slice(1);
        break;
      case '*':
        filterKind = (item) => item.kind === 'server';
        needle = needle.slice(1);
        break;
    }
  }

  const recency = (item: SwitcherItem): number => {
    if (item.kind !== 'channel') return recent.length;
    const index = recent.findIndex(
      (visit) => visit.server === item.serverKey && String(visit.channel) === String(item.channelId),
    );
    return index === -1 ? recent.length : index;
  };

  const kindOrder = (item: SwitcherItem): number => {
    switch (item.kind) {
      case 'channel':
        return 0;
      case 'server':
        return 1;
      case 'member':
        return 2;
    }
  };

  const scored: [number, SwitcherItem][] = [];
  for (const item of items) {
    if (!filterKind(item)) continue;
    const score = needle.length === 0 ? 0 : scoreItem(item.name, needle);
    if (score !== null) {
      scored.push([score, item]);
    }
  }

  scored.sort((a, b) => {
    if (a[0] !== b[0]) return a[0] - b[0];
    if (needle.length === 0) {
      const byRecency = recency(a[1]) - recency(b[1]);
      if (byRecency !== 0) return byRecency;
    }
    const byKind = kindOrder(a[1]) - kindOrder(b[1]);
    if (byKind !== 0) return byKind;
    return a[1].name.toLowerCase().localeCompare(b[1].name.toLowerCase());
  });

  return scored.slice(0, SWITCHER_LIMIT).map((pair) => pair[1]);
}

/** Collects channels visible across all servers, the servers, and members of the active server. */
export function collectSwitcherItems(state: {
  servers: AppState['servers'];
  order: AppState['order'];
  serverInFront?: AppState['serverInFront'];
}): SwitcherItem[] {
  const items: SwitcherItem[] = [];

  for (const serverKey of state.order) {
    const serverState = state.servers[serverKey];
    const snapshot = serverState?.snapshot;
    if (!snapshot) continue;

    items.push({
      kind: 'server',
      serverKey: snapshot.serverKey,
      name: snapshot.name,
      iconHash: snapshot.iconHash,
    });

    const accessMap = new Map(snapshot.access.map((a) => [a.channelId, a]));

    for (const ch of snapshot.channels) {
      if (ch.kind === ChannelKind.Category) continue;
      const access = accessMap.get(ch.id);
      if (access && !access.view) continue;

      items.push({
        kind: 'channel',
        serverKey: snapshot.serverKey,
        serverName: snapshot.name,
        channelId: ch.id,
        name: ch.name,
        channelKind: ch.kind,
      });
    }
  }

  const activeServerKey = state.serverInFront ?? state.order[0];
  const activeSnapshot = activeServerKey ? state.servers[activeServerKey]?.snapshot : null;
  if (activeSnapshot) {
    for (const member of activeSnapshot.members) {
      items.push({
        kind: 'member',
        serverKey: activeSnapshot.serverKey,
        userId: member.user.id,
        name: member.user.displayName,
        nickname: member.nickname,
        avatarHash: member.user.avatarHash,
      });
    }
  }

  return items;
}
