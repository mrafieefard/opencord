import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/suggestions.dart';

import '../../support/fixtures.dart';

void main() {
  final data = ServerData.fromSnapshot(snapshot());
  final general = data.channels[generalId]!;

  List<String> labels(List<Suggestion> list) => [
    for (final suggestion in list) suggestion.insert.trim(),
  ];

  test('members match from the start of any word, the name start first', () {
    final found = suggest(data, general, (trigger: '@', query: 'a', start: 0));

    // Alex starts with A; Kai and Mira only contain it.
    expect(labels(found), ['@Alex']);
    expect(
      labels(suggest(data, general, (trigger: '@', query: '', start: 0))),
      ['@Alex', '@Kai', '@Mira'],
    );
  });

  test('channels the reader can see match by name', () {
    expect(
      labels(suggest(data, general, (trigger: '#', query: 'ra', start: 0))),
      ['#random'],
    );
  });

  test('emoji come from the shortcode search', () {
    final found = suggest(data, general, (
      trigger: ':',
      query: 'thumbsup',
      start: 0,
    ));
    expect(found.first.insert, '👍 ');
  });

  test('at most eight come back', () {
    expect(
      suggest(data, general, (trigger: ':', query: 'fa', start: 0)),
      hasLength(8),
    );
  });

  test('channels outside view are left out', () {
    final hidden = data.copyWith(
      channels: {
        ...data.channels,
        77: const Channel(id: 77, kind: ChannelKind.text, name: 'rabbit'),
      },
      channelPermissions: {...data.channelPermissions},
    );
    expect(
      labels(suggest(hidden, general, (trigger: '#', query: 'ra', start: 0))),
      ['#random'],
    );
  });
}
