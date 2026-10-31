import {
  MdChannelMention,
  MdCode,
  MdInline,
  MdInlineCode,
  MdLink,
  MdParagraph,
  MdQuote,
  MdText,
  MdUserMention,
  endsWithCode,
  parseInline,
  parseMarkdown,
} from '../markdown';

function inlines(source: string): MdInline[] {
  const blocks = parseMarkdown(source);
  expect(blocks).toHaveLength(1);
  return (blocks[0] as MdParagraph).inlines;
}

describe('markdown', () => {
  it('plain text stays one text span', () => {
    expect(inlines('hello world')).toEqual([new MdText('hello world')]);
  });

  it('bold, italic and strike, nested', () => {
    expect(inlines('**bold** and *it* and _it_ and ~~gone~~')).toEqual([
      new MdText('bold', { bold: true }),
      new MdText(' and '),
      new MdText('it', { italic: true }),
      new MdText(' and '),
      new MdText('it', { italic: true }),
      new MdText(' and '),
      new MdText('gone', { strike: true }),
    ]);
    expect(inlines('**bold _both_**')).toEqual([
      new MdText('bold ', { bold: true }),
      new MdText('both', { bold: true, italic: true }),
    ]);
  });

  it('underscores inside words are not italics', () => {
    expect(inlines('call snake_case_name now')).toEqual([
      new MdText('call snake_case_name now'),
    ]);
  });

  it('unmatched markers stay literal', () => {
    expect(inlines('2 * 3 = 6 and **open')).toEqual([
      new MdText('2 * 3 = 6 and **open'),
    ]);
  });

  it('inline code is taken literally', () => {
    expect(inlines('run `cargo **test**` now')).toEqual([
      new MdText('run '),
      new MdInlineCode('cargo **test**'),
      new MdText(' now'),
    ]);
  });

  it('mentions, channel links and web links', () => {
    expect(inlines('hi <@1000>, see <#10> and https://example.org/a?b=c.')).toEqual([
      new MdText('hi '),
      new MdUserMention(1000),
      new MdText(', see '),
      new MdChannelMention(10),
      new MdText(' and '),
      new MdLink('https://example.org/a?b=c'),
      new MdText('.'),
    ]);
  });

  it('opencord links are links too', () => {
    expect(inlines('open opencord://home.example:7710/c/10/5, now')).toEqual([
      new MdText('open '),
      new MdLink('opencord://home.example:7710/c/10/5'),
      new MdText(', now'),
    ]);
  });

  it('ids too big for 64 bits are not mentions', () => {
    expect(parseInline('hi <@99999999999999999999> <#9223372036854775808>')).toEqual([
      new MdText('hi <@99999999999999999999> <#9223372036854775808>'),
    ]);
  });

  it('a backslash escapes a marker', () => {
    expect(inlines('not \\*italic\\*')).toEqual([new MdText('not *italic*')]);
  });

  it('fenced code blocks keep their language and text', () => {
    const blocks = parseMarkdown(
      'Look:\n```rust\nlet x = 1;\n  indented\n```\nafter',
    );

    expect(blocks).toEqual([
      new MdParagraph([new MdText('Look:')]),
      new MdCode('let x = 1;\n  indented', 'rust'),
      new MdParagraph([new MdText('after')]),
    ]);
  });

  it('an unclosed fence is plain text', () => {
    expect(parseMarkdown('```\nno end')).toEqual([
      new MdParagraph([new MdText('```\nno end')]),
    ]);
  });

  it('quotes are lines starting with >', () => {
    expect(parseMarkdown('> quoted **text**\n> more\nanswer')).toEqual([
      new MdQuote([
        new MdText('quoted '),
        new MdText('text', { bold: true }),
        new MdText('\nmore'),
      ]),
      new MdParagraph([new MdText('answer')]),
    ]);
  });

  it('a message that ends with a code block says so', () => {
    expect(endsWithCode(parseMarkdown('x\n```\ncode\n```'))).toBe(true);
    expect(endsWithCode(parseMarkdown('x'))).toBe(false);
  });

  it('line-break case a\\nb (two lines in one paragraph)', () => {
    const blocks = parseMarkdown('a\nb');
    expect(blocks).toEqual([
      new MdParagraph([new MdText('a\nb')]),
    ]);
  });
});
