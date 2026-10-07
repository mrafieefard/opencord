import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/markdown.dart';

List<MdInline> inlines(String source) {
  final blocks = parseMarkdown(source);
  expect(blocks, hasLength(1));
  return (blocks.single as MdParagraph).inlines;
}

void main() {
  test('plain text stays one text span', () {
    expect(inlines('hello world'), [const MdText('hello world')]);
  });

  test('bold, italic and strike, nested', () {
    expect(inlines('**bold** and *it* and _it_ and ~~gone~~'), [
      const MdText('bold', bold: true),
      const MdText(' and '),
      const MdText('it', italic: true),
      const MdText(' and '),
      const MdText('it', italic: true),
      const MdText(' and '),
      const MdText('gone', strike: true),
    ]);
    expect(inlines('**bold _both_**'), [
      const MdText('bold ', bold: true),
      const MdText('both', bold: true, italic: true),
    ]);
  });

  test('underscores inside words are not italics', () {
    expect(inlines('call snake_case_name now'), [
      const MdText('call snake_case_name now'),
    ]);
  });

  test('unmatched markers stay literal', () {
    expect(inlines('2 * 3 = 6 and **open'), [
      const MdText('2 * 3 = 6 and **open'),
    ]);
  });

  test('inline code is taken literally', () {
    expect(inlines('run `cargo **test**` now'), [
      const MdText('run '),
      const MdInlineCode('cargo **test**'),
      const MdText(' now'),
    ]);
  });

  test('mentions, channel links and web links', () {
    expect(inlines('hi <@1000>, see <#10> and https://example.org/a?b=c.'), [
      const MdText('hi '),
      const MdUserMention(1000),
      const MdText(', see '),
      const MdChannelMention(10),
      const MdText(' and '),
      const MdLink('https://example.org/a?b=c'),
      const MdText('.'),
    ]);
  });

  test('ids too big for 64 bits are not mentions', () {
    expect(parseInline('hi <@99999999999999999999> <#9223372036854775808>'), [
      const MdText('hi <@99999999999999999999> <#9223372036854775808>'),
    ]);
  });

  test('a backslash escapes a marker', () {
    expect(inlines(r'not \*italic\*'), [const MdText('not *italic*')]);
  });

  test('fenced code blocks keep their language and text', () {
    final blocks = parseMarkdown(
      "Look:\n```rust\nlet x = 1;\n  indented\n```\nafter",
    );

    expect(blocks, [
      const MdParagraph([MdText('Look:')]),
      const MdCode('let x = 1;\n  indented', language: 'rust'),
      const MdParagraph([MdText('after')]),
    ]);
  });

  test('an unclosed fence is plain text', () {
    expect(parseMarkdown('```\nno end'), [
      const MdParagraph([MdText('```\nno end')]),
    ]);
  });

  test('quotes are lines starting with >', () {
    expect(parseMarkdown('> quoted **text**\n> more\nanswer'), [
      const MdQuote([
        MdText('quoted '),
        MdText('text', bold: true),
        MdText('\nmore'),
      ]),
      const MdParagraph([MdText('answer')]),
    ]);
  });

  test('a message that ends with a code block says so', () {
    expect(endsWithCode(parseMarkdown('x\n```\ncode\n```')), isTrue);
    expect(endsWithCode(parseMarkdown('x')), isFalse);
  });
}
