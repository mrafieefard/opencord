// The markdown subset of design §8.5, parsed by our own small parser:
// paragraphs, `> quotes` and fenced code blocks, with **bold**, *italic* /
// _italic_, ~~strike~~, `inline code`, links, `<@user>` mentions and `<#channel>`
// links inside them. Port of app/lib/features/chat/markdown.dart.

export class MdParagraph {
  constructor(public inlines: MdInline[]) {}
}

export class MdQuote {
  constructor(public inlines: MdInline[]) {}
}

export class MdCode {
  constructor(public code: string, public language?: string) {}
}

export class MdText {
  public bold: boolean;
  public italic: boolean;
  public strike: boolean;

  constructor(
    public text: string,
    options?: { bold?: boolean; italic?: boolean; strike?: boolean },
  ) {
    this.bold = options?.bold ?? false;
    this.italic = options?.italic ?? false;
    this.strike = options?.strike ?? false;
  }
}

export class MdInlineCode {
  constructor(public code: string) {}
}

export class MdLink {
  constructor(public url: string) {}
}

export class MdUserMention {
  constructor(public userId: number | string) {}
}

export class MdChannelMention {
  constructor(public channelId: number | string) {}
}

export type MdBlock = MdParagraph | MdQuote | MdCode;
export type MdInline = MdText | MdInlineCode | MdLink | MdUserMention | MdChannelMention;

/** Whether the message ends in a code block (§8.5). */
export function endsWithCode(blocks: MdBlock[]): boolean {
  return blocks.length > 0 && blocks[blocks.length - 1] instanceof MdCode;
}

const URL_REGEX = /^(?:https?|opencord):\/\/[^\s<>"]+/;
const ESCAPABLE = '\\*_~`<>#@';
const TRAILING = '.,!?;:\'")';
const USER_MENTION_REGEX = /^<@(\d+)>/;
const CHANNEL_MENTION_REGEX = /^<#(\d+)>/;

const isWord = (char: string): boolean => /\p{L}|\p{N}/u.test(char);

function mentionId(digits: string): number | null {
  try {
    const val = BigInt(digits);
    if (val >= 0n && val <= 9223372036854775807n) {
      return Number(val);
    }
  } catch {
    return null;
  }
  return null;
}

function closing(text: string, open: number, token: string): number {
  const start = open + token.length;
  const startChar = text[start];
  if (start >= text.length || !startChar || startChar.trim() === '') return -1;
  const prevOpen = text[open - 1];
  if (token === '_' && open > 0 && prevOpen !== undefined && isWord(prevOpen)) return -1;
  for (let j = start + 1; j <= text.length - token.length; j++) {
    if (!text.startsWith(token, j)) continue;
    const prevJ = text[j - 1];
    if (!prevJ || prevJ.trim() === '') continue;
    if (token === '*' && (text.startsWith('**', j) || prevJ === '*')) {
      continue;
    }
    const after = j + token.length;
    const afterChar = text[after];
    if (token === '_' && after < text.length && afterChar !== undefined && isWord(afterChar)) continue;
    return j;
  }
  return -1;
}

export function parseInline(
  text: string,
  options?: { bold?: boolean; italic?: boolean; strike?: boolean },
): MdInline[] {
  const bold = options?.bold ?? false;
  const italic = options?.italic ?? false;
  const strike = options?.strike ?? false;

  const result: MdInline[] = [];
  let buffer = '';

  const flush = () => {
    if (buffer.length === 0) return;
    result.push(new MdText(buffer, { bold, italic, strike }));
    buffer = '';
  };

  let i = 0;
  while (i < text.length) {
    const char = text[i];
    if (!char) {
      i++;
      continue;
    }
    const nextChar = text[i + 1];
    if (char === '\\' && nextChar !== undefined && ESCAPABLE.includes(nextChar)) {
      buffer += nextChar;
      i += 2;
      continue;
    }
    if (char === '`') {
      const end = text.indexOf('`', i + 1);
      if (end > i + 1) {
        flush();
        result.push(new MdInlineCode(text.substring(i + 1, end)));
        i = end + 1;
        continue;
      }
    }
    if (char === '<') {
      const rest = text.substring(i);
      const userMatch = rest.match(USER_MENTION_REGEX);
      const channelMatch = rest.match(CHANNEL_MENTION_REGEX);
      const match = userMatch ?? channelMatch;
      if (match && match[1] !== undefined) {
        const id = mentionId(match[1]);
        if (id !== null) {
          flush();
          result.push(userMatch ? new MdUserMention(id) : new MdChannelMention(id));
          i += match[0].length;
          continue;
        }
      }
    }
    const prevChar = text[i - 1];
    if ((char === 'h' || char === 'o') && (i === 0 || (prevChar !== undefined && !isWord(prevChar)))) {
      const rest = text.substring(i);
      const linkMatch = rest.match(URL_REGEX);
      if (linkMatch) {
        let url = linkMatch[0];
        let lastChar = url[url.length - 1];
        while (url.length > 0 && lastChar !== undefined && TRAILING.includes(lastChar)) {
          if (
            url.endsWith(')') &&
            (url.match(/\(/g) || []).length >= (url.match(/\)/g) || []).length
          ) {
            break;
          }
          url = url.substring(0, url.length - 1);
          lastChar = url[url.length - 1];
        }
        flush();
        result.push(new MdLink(url));
        i += url.length;
        continue;
      }
    }
    const token = text.startsWith('**', i)
      ? '**'
      : text.startsWith('~~', i)
      ? '~~'
      : char === '*' || char === '_'
      ? char
      : null;
    if (token !== null) {
      const close = closing(text, i, token);
      if (close !== -1) {
        flush();
        result.push(
          ...parseInline(text.substring(i + token.length, close), {
            bold: bold || token === '**',
            italic: italic || token === '*' || token === '_',
            strike: strike || token === '~~',
          }),
        );
        i = close + token.length;
        continue;
      }
      buffer += token;
      i += token.length;
      continue;
    }
    buffer += char;
    i++;
  }
  flush();
  return result;
}

export interface Fence {
  start: number;
  end: number;
  language?: string;
  code: string;
}

const LANG_PATTERN = /^[A-Za-z0-9_+#.-]{1,32}$/;

export function findFences(text: string): Fence[] {
  const fences: Fence[] = [];
  let cursor = 0;

  while (cursor < text.length) {
    const lineStart = cursor;
    let i = lineStart;
    while (i < text.length && (text[i] === ' ' || text[i] === '\t')) {
      i++;
    }

    if (i + 3 <= text.length && text.startsWith('```', i)) {
      const openFenceEnd = i + 3;
      const closeStart = text.indexOf('```', openFenceEnd);
      if (closeStart !== -1) {
        const raw = text.substring(openFenceEnd, closeStart);
        const newlineIndex = raw.indexOf('\n');
        let language: string | undefined;
        let codeRaw: string;

        if (newlineIndex !== -1) {
          let langCandidate = raw.substring(0, newlineIndex);
          if (langCandidate.endsWith('\r')) {
            langCandidate = langCandidate.substring(0, langCandidate.length - 1);
          }
          if (LANG_PATTERN.test(langCandidate)) {
            language = langCandidate;
            codeRaw = raw.substring(newlineIndex + 1);
          } else {
            language = undefined;
            codeRaw = raw;
          }
        } else {
          language = undefined;
          codeRaw = raw;
        }

        const codeLines = codeRaw.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
        let startLine = 0;
        while (startLine < codeLines.length && (codeLines[startLine] ?? '').trim().length === 0) {
          startLine++;
        }
        let endLine = codeLines.length - 1;
        while (endLine >= startLine && (codeLines[endLine] ?? '').trim().length === 0) {
          endLine--;
        }

        if (startLine <= endLine) {
          const code = codeLines.slice(startLine, endLine + 1).join('\n');
          if (code.length > 0) {
            const closeEnd = closeStart + 3;
            fences.push({
              start: lineStart,
              end: closeEnd,
              language,
              code,
            });
            const nextNewline = text.indexOf('\n', closeEnd);
            if (nextNewline !== -1) {
              cursor = nextNewline + 1;
            } else {
              cursor = text.length;
            }
            continue;
          }
        }
      }
    }

    const nextNewline = text.indexOf('\n', cursor);
    if (nextNewline !== -1) {
      cursor = nextNewline + 1;
    } else {
      break;
    }
  }

  return fences;
}

export function parseMarkdown(source: string): MdBlock[] {
  const fences = findFences(source);
  if (fences.length === 0) {
    return parseNonCodeBlocks(source);
  }

  const blocks: MdBlock[] = [];
  let cursor = 0;

  for (const fence of fences) {
    if (fence.start > cursor) {
      const textBefore = source.substring(cursor, fence.start);
      blocks.push(...parseNonCodeBlocks(textBefore));
    }
    blocks.push(new MdCode(fence.code, fence.language));
    cursor = fence.end;
  }

  if (cursor < source.length) {
    const textAfter = source.substring(cursor);
    blocks.push(...parseNonCodeBlocks(textAfter));
  }

  return blocks;
}

function parseNonCodeBlocks(source: string): MdBlock[] {
  let text = source;
  while (text.startsWith('\n') || text.startsWith('\r')) {
    text = text.substring(1);
  }
  while (text.endsWith('\n') || text.endsWith('\r')) {
    text = text.substring(0, text.length - 1);
  }
  if (text.trim().length === 0) return [];

  const lines = text.split('\n');
  const blocks: MdBlock[] = [];
  const paragraph: string[] = [];
  const quote: string[] = [];

  const flushParagraph = () => {
    if (paragraph.length === 0) return;
    const joined = paragraph.join('\n');
    if (joined.trim().length > 0) {
      let content = joined;
      if (content.startsWith(' ') && !content.startsWith('    ')) {
        content = content.trimStart();
      }
      blocks.push(new MdParagraph(parseInline(content)));
    }
    paragraph.length = 0;
  };

  const flushQuote = () => {
    if (quote.length === 0) return;
    const joined = quote.join('\n');
    if (joined.trim().length > 0) {
      blocks.push(new MdQuote(parseInline(joined)));
    }
    quote.length = 0;
  };

  for (const line of lines) {
    if (line.startsWith('>')) {
      flushParagraph();
      const rest = line.substring(1);
      quote.push(rest.startsWith(' ') ? rest.substring(1) : rest);
    } else {
      flushQuote();
      paragraph.push(line);
    }
  }
  flushParagraph();
  flushQuote();
  return blocks;
}
