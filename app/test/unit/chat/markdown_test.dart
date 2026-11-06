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

  test('opencord links are links too', () {
    expect(inlines('open opencord://home.example:7710/c/10/5, now'), [
      const MdText('open '),
      const MdLink('opencord://home.example:7710/c/10/5'),
      const MdText(', now'),
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

  group('spec §7.1 fence table', () {
    test('table row 1: ```py⏎⏎CODE```', () {
      expect(parseMarkdown('```py\n\nCODE```'), [
        const MdCode('CODE', language: 'py'),
      ]);
    });

    test('table row 2: ```js⏎Code⏎```', () {
      expect(parseMarkdown('```js\nCode\n```'), [
        const MdCode('Code', language: 'js'),
      ]);
    });

    test('table row 3: ```one line```', () {
      expect(parseMarkdown('```one line```'), [const MdCode('one line')]);
    });

    test('table row 4: ```py code```', () {
      expect(parseMarkdown('```py code```'), [const MdCode('py code')]);
    });

    test('table row 5: ```⏎b``` after', () {
      expect(parseMarkdown('```\nb``` after'), [
        const MdCode('b'),
        const MdParagraph([MdText('after')]),
      ]);
    });

    test('table row 6: ```⏎no end', () {
      expect(parseMarkdown('```\nno end'), [
        const MdParagraph([MdText('```\nno end')]),
      ]);
    });
  });

  test('a_language_can_have_these_characters', () {
    expect(parseMarkdown('```c++\ncode```'), [
      const MdCode('code', language: 'c++'),
    ]);
    expect(parseMarkdown('```c#\ncode```'), [
      const MdCode('code', language: 'c#'),
    ]);
    expect(parseMarkdown('```objective-c\ncode```'), [
      const MdCode('code', language: 'objective-c'),
    ]);
    expect(parseMarkdown('```f.x\ncode```'), [
      const MdCode('code', language: 'f.x'),
    ]);

    final longLang = 'a' * 33;
    expect(parseMarkdown('```$longLang\ncode```'), [MdCode('$longLang\ncode')]);
  });

  test('indentation_and_blank_lines_inside_stay', () {
    const input = '```py\n  def foo():\n\n    return 42\n```';
    expect(parseMarkdown(input), [
      const MdCode('  def foo():\n\n    return 42', language: 'py'),
    ]);
  });

  test('blank_lines_at_the_ends_go', () {
    const input = '```py\n\n\ncode\n\n\n```';
    expect(parseMarkdown(input), [const MdCode('code', language: 'py')]);
  });

  test('an_opening_fence_mid_line_is_not_a_block', () {
    final blocks = parseMarkdown('see ```x``` here');
    expect(blocks, hasLength(1));
    expect(blocks.single, isA<MdParagraph>());
  });

  test('two_blocks_in_one_message', () {
    const input = '```py\nx = 1\n```\n\n```js\ny = 2\n```';
    expect(parseMarkdown(input), [
      const MdCode('x = 1', language: 'py'),
      const MdCode('y = 2', language: 'js'),
    ]);
  });

  test('empty_fences_stay_text', () {
    expect(parseMarkdown('``````').single, isA<MdParagraph>());
    expect(parseMarkdown('```\n```').single, isA<MdParagraph>());
    expect(parseMarkdown('```   ```').single, isA<MdParagraph>());
  });

  test('text_before_and_after_a_block', () {
    const input = 'before\n```py\nx = 1\n```\nafter';
    expect(parseMarkdown(input), [
      const MdParagraph([MdText('before')]),
      const MdCode('x = 1', language: 'py'),
      const MdParagraph([MdText('after')]),
    ]);
  });
}
