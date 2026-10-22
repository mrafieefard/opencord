import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/links/app_links.dart';

void main() {
  group('opencord:// links (§15)', () {
    test('an invite keeps the whole link for the Add server dialog', () {
      const link = 'opencord://chat.example.org:7710/invite/AbC123#fp=00ff';

      expect(parseAppLink(link), const InviteAppLink(link));
    });

    test('a channel link names its server and channel', () {
      expect(
        parseAppLink('opencord://opencord.example:7710/c/10002'),
        const ChannelAppLink('opencord.example:7710', 10002),
      );
    });

    test('a message link adds the message', () {
      expect(
        parseAppLink(' opencord://opencord.example:7710/c/10002/55 '),
        const ChannelAppLink('opencord.example:7710', 10002, 55),
      );
    });

    test('other text is not an app link', () {
      for (final text in [
        'https://opencord.example/c/1',
        'opencord://opencord.example:7710/',
        'opencord://opencord.example:7710/c/general',
        'opencord://opencord.example:7710/c/1/2/3',
        'opencord://opencord.example:7710/settings',
        'opencord:',
        '',
      ]) {
        expect(parseAppLink(text), isNull, reason: text);
      }
    });
  });
}
