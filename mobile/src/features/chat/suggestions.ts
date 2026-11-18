// Composer suggestions (design §4.6, P2 Task 4): a port of suggest in
// desktop suggestions.dart. @ lists members who can see the channel with their
// highest role; # lists text channels; : lists emoji.
import { ChannelKind, type ChannelRecord, type MemberRecord, type ServerSnapshot } from '@opencord/core-native';

import { searchEmoji, type Emoji } from '@/features/emoji/emoji';
import type { ActiveToken } from './mentions';

export type Suggestion =
  | { kind: 'member'; member: MemberRecord; role?: string; insert: string }
  | { kind: 'channel'; channel: ChannelRecord; insert: string }
  | { kind: 'emoji'; emoji: Emoji; insert: string };

export const maxSuggestions = 8;

/** 0 when name starts with query, 1 when one of its words does. */
function rank(name: string, query: string): number | null {
  const lower = name.toLowerCase();
  if (lower.startsWith(query)) return 0;
  const words = lower.split(/[\s\-_]/);
  if (words.some((word) => word.startsWith(query))) {
    return 1;
  }
  return null;
}

function best<T>(items: readonly T[], nameFn: (item: T) => string, query: string): T[] {
  const ranked: { rank: number; item: T; name: string }[] = [];
  for (const item of items) {
    const n = nameFn(item);
    const r = rank(n, query);
    if (r !== null) {
      ranked.push({ rank: r, item, name: n.toLowerCase() });
    }
  }
  ranked.sort((a, b) => {
    if (a.rank !== b.rank) return a.rank - b.rank;
    return a.name.localeCompare(b.name);
  });
  return ranked.slice(0, maxSuggestions).map((r) => r.item);
}

function highestRoleName(snapshot: ServerSnapshot, member: MemberRecord): string | undefined {
  if (!snapshot.roles || snapshot.roles.length === 0) return undefined;
  const roles = snapshot.roles
    .filter((r) => member.roleIds.includes(r.id) && r.position > 0)
    .sort((a, b) => b.position - a.position);
  return roles[0]?.name;
}

export function suggest(
  snapshot: ServerSnapshot | null | undefined,
  _channelId: string,
  token: ActiveToken,
): Suggestion[] {
  if (!snapshot) return [];
  const query = token.query.toLowerCase();

  switch (token.trigger) {
    case '@': {
      const matched = best(
        snapshot.members,
        (m) => m.nickname ?? m.user.displayName,
        query,
      );
      return matched.map((m) => {
        const displayName = m.nickname ?? m.user.displayName;
        return {
          kind: 'member' as const,
          member: m,
          role: highestRoleName(snapshot, m),
          insert: `@${displayName} `,
        };
      });
    }
    case '#': {
      const visible = snapshot.channels.filter((c) => {
        if (c.kind !== ChannelKind.Text) return false;
        const channelAccess = snapshot.access?.find((a) => a.channelId === c.id);
        return channelAccess ? channelAccess.view : true;
      });
      const matched = best(visible, (c) => c.name, query);
      return matched.map((c) => ({
        kind: 'channel' as const,
        channel: c,
        insert: `#${c.name} `,
      }));
    }
    case ':': {
      const matched = searchEmoji(token.query, maxSuggestions);
      return matched.map((emoji) => ({
        kind: 'emoji' as const,
        emoji,
        insert: `${emoji.char} `,
      }));
    }
  }
}

/** Replaces the active token in text with suggestion.insert. */
export function applySuggestion(
  text: string,
  caret: number,
  token: ActiveToken,
  suggestion: Suggestion,
): { text: string; caret: number } {
  const before = text.slice(0, token.start);
  const after = text.slice(caret);
  const newText = before + suggestion.insert + after;
  const newCaret = before.length + suggestion.insert.length;
  return { text: newText, caret: newCaret };
}
