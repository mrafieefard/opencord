import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/mentions.dart';
import 'package:opencord/features/chat/message_actions.dart';

const members = {1001: 'Kai Nakamura', 1002: 'Kai', 1003: 'Mira'};
const channels = {10: 'general', 11: 'dev-core'};

String encode(String text) =>
    encodeMentions(text, members: members, channels: channels);

String decode(String content) =>
    decodeMentions(content, members: members, channels: channels);

void main() {
  group('encode', () {
    test('names become mention tokens, the longest name first', () {
      expect(encode('hi @Kai Nakamura and @Kai!'), 'hi <@1001> and <@1002>!');
      expect(encode('see #dev-core or #general.'), 'see <#11> or <#10>.');
    });

    test('a name has to end at a word boundary', () {
      expect(encode('@Miranda says hi'), '@Miranda says hi');
      expect(encode('email@Mira'), 'email@Mira');
    });

    test('code is left alone', () {
      expect(encode('`@Mira` and @Mira'), '`@Mira` and <@1003>');
      expect(encode('```\n@Mira #general\n```'), '```\n@Mira #general\n```');
    });

    test('unknown names stay as typed', () {
      expect(encode('@Nobody in #nowhere'), '@Nobody in #nowhere');
    });
  });

  test('ids too big for 64 bits stay as typed', () {
    const typed = '<@99999999999999999999> <#99999999999999999999>';
    expect(decode(typed), typed);
    expect(
      copyableText(typed, user: (_) => 'Kai', channel: (_) => 'general'),
      typed,
    );
  });

  test('decode writes tokens back as names, for editing', () {
    expect(decode('<@1001> see <#11>'), '@Kai Nakamura see #dev-core');
    expect(decode('<@9> <#9>'), '<@9> <#9>');
    expect(encode(decode('hi <@1003> in <#10>')), 'hi <@1003> in <#10>');
  });

  group('active token', () {
    test('finds what is being typed after @, # or :', () {
      expect(activeToken('hello @Ka', 9), (
        trigger: '@',
        query: 'Ka',
        start: 6,
      ));
      expect(activeToken('#gen', 4), (trigger: '#', query: 'gen', start: 0));
      expect(activeToken('nice :thu', 9), (
        trigger: ':',
        query: 'thu',
        start: 5,
      ));
    });

    test('member names may contain spaces', () {
      expect(activeToken('@Kai Nak', 8), (
        trigger: '@',
        query: 'Kai Nak',
        start: 0,
      ));
    });

    test('needs a word start, and two letters for emoji', () {
      expect(activeToken('a@b', 3), isNull);
      expect(activeToken('at 10:30', 8), isNull);
      expect(activeToken('nice :t', 7), isNull);
      expect(activeToken('#gen eral', 9), isNull);
    });

    test('stops at a new line', () {
      expect(activeToken('@Kai\nthere', 10), isNull);
    });
  });
}
