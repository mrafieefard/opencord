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

export function parseMarkdown(source: string): MdBlock[] {
  const lines = source.split('\n');
  const blocks: MdBlock[] = [];
  const paragraph: string[] = [];
  const quote: string[] = [];

  const flushParagraph = () => {
    if (paragraph.length === 0) return;
    blocks.push(new MdParagraph(parseInline(paragraph.join('\n'))));
    paragraph.length = 0;
  };

  const flushQuote = () => {
    if (quote.length === 0) return;
    blocks.push(new MdQuote(parseInline(quote.join('\n'))));
    quote.length = 0;
  };

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line === undefined) {
      i++;
      continue;
    }
    if (line.trimStart().startsWith('```')) {
      const end = closingFence(lines, i + 1);
      if (end !== -1) {
        flushParagraph();
        flushQuote();
        const language = line.trim().substring(3).trim();
        blocks.push(
          new MdCode(
            lines.slice(i + 1, end).join('\n'),
            language.length === 0 ? undefined : language,
          ),
        );
        i = end + 1;
        continue;
      }
    }
    if (line.startsWith('>')) {
      flushParagraph();
      const rest = line.substring(1);
      quote.push(rest.startsWith(' ') ? rest.substring(1) : rest);
    } else {
      flushQuote();
      paragraph.push(line);
    }
    i++;
  }
  flushParagraph();
  flushQuote();
  return blocks;
}

function closingFence(lines: string[], from: number): number {
  for (let j = from; j < lines.length; j++) {
    const line = lines[j];
    if (line !== undefined && line.trim() === '```') return j;
  }
  return -1;
}
