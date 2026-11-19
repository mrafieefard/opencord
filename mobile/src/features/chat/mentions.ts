/// Mentions as people type them and as they are sent (§4.6): the composer
/// shows "@Kai Nakamura" and "#general"; messages carry `<@1001>` and
/// `<#10>`, which every client renders by name.

export interface ActiveToken {
  trigger: '@' | '#' | ':';
  query: string;
  start: number;
}

const CODE_PATTERN = /```[\s\S]*?```|`[^`\n]*`/g;

function outsideCode(text: string, convert: (part: string) => string): string {
  let result = '';
  let last = 0;
  for (const match of text.matchAll(CODE_PATTERN)) {
    const start = match.index ?? 0;
    result += convert(text.substring(last, start));
    result += match[0];
    last = start + match[0].length;
  }
  result += convert(text.substring(last));
  return result;
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function namePattern(
  prefix: string,
  names: Iterable<string>,
  extraWordChars: string,
): RegExp | null {
  const sorted = Array.from(names)
    .filter((name) => name.length > 0)
    .sort((a, b) => b.length - a.length);
  if (sorted.length === 0) return null;
  const alternatives = sorted.map(escapeRegex).join('|');
  return new RegExp(
    `(?<![\\p{L}\\p{N}_])${prefix}(${alternatives})(?![\\p{L}\\p{N}_${extraWordChars}])`,
    'gu',
  );
}

function invertMap(map: Map<string, string>): Map<string, string> {
  const inverted = new Map<string, string>();
  for (const [id, name] of map.entries()) {
    if (!inverted.has(name)) {
      inverted.set(name, id);
    }
  }
  return inverted;
}

const USER_MENTION_TOKEN = /<@(\d+)>/g;
const CHANNEL_MENTION_TOKEN = /<#(\d+)>/g;

const MAX_I64 = 9223372036854775807n;

function isValidId(digits: string): boolean {
  if (digits.length > 19) return false;
  try {
    const val = BigInt(digits);
    return val >= 0n && val <= MAX_I64;
  } catch {
    return false;
  }
}

function replaceMentionTokens(
  content: string,
  user: (id: string, token: string) => string,
  channel: (id: string, token: string) => string,
): string {
  let res = content.replace(USER_MENTION_TOKEN, (token, digits: string) => {
    if (isValidId(digits)) {
      return user(digits, token);
    }
    return token;
  });
  res = res.replace(CHANNEL_MENTION_TOKEN, (token, digits: string) => {
    if (isValidId(digits)) {
      return channel(digits, token);
    }
    return token;
  });
  return res;
}

export function encodeMentions(
  text: string,
  members: Map<string, string>,
  channels: Map<string, string>,
): string {
  const memberIds = invertMap(members);
  const channelIds = invertMap(channels);
  const memberPattern = namePattern('@', memberIds.keys(), '');
  const channelPattern = namePattern('#', channelIds.keys(), '\\-');

  return outsideCode(text, (part) => {
    let result = part;
    if (memberPattern !== null) {
      result = result.replace(memberPattern, (_match, name: string) => {
        const id = memberIds.get(name);
        return id !== undefined ? `<@${id}>` : _match;
      });
    }
    if (channelPattern !== null) {
      result = result.replace(channelPattern, (_match, name: string) => {
        const id = channelIds.get(name);
        return id !== undefined ? `<#${id}>` : _match;
      });
    }
    return result;
  });
}

export function decodeMentions(
  content: string,
  members: Map<string, string>,
  channels: Map<string, string>,
): string {
  return outsideCode(content, (part) =>
    replaceMentionTokens(
      part,
      (id, token) => {
        const name = members.get(id);
        return name !== undefined ? `@${name}` : token;
      },
      (id, token) => {
        const name = channels.get(id);
        return name !== undefined ? `#${name}` : token;
      },
    ),
  );
}

function startsWord(text: string, index: number): boolean {
  if (index === 0) return true;
  const prev = text[index - 1];
  return prev !== undefined && /[\s(\["']/.test(prev);
}

export function activeToken(text: string, caret: number): ActiveToken | null {
  if (caret < 0 || caret > text.length) return null;
  const before = text.substring(0, caret);
  const lineStart = before.lastIndexOf('\n') + 1;
  const line = before.substring(lineStart);
  for (let i = line.length - 1; i >= 0; i--) {
    const trigger = line[i];
    if (trigger !== '@' && trigger !== '#' && trigger !== ':') continue;
    if (!startsWord(line, i)) continue;
    const query = line.substring(i + 1);
    if (query.startsWith(' ')) return null;
    const spaced = /\s/.test(query);
    if (trigger === '@' && query.length <= 32) {
      return { trigger, query, start: lineStart + i };
    }
    if (trigger === '#' && !spaced) {
      return { trigger, query, start: lineStart + i };
    }
    if (trigger === ':' && !spaced && query.length >= 2) {
      return { trigger, query, start: lineStart + i };
    }
    return null;
  }
  return null;
}
