import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_layout.dart';

const channels = [
  Channel(id: 1, kind: ChannelKind.category, name: 'Text', position: 1),
  Channel(id: 2, kind: ChannelKind.category, name: 'Info', position: 0),
  Channel(
    id: 10,
    kind: ChannelKind.text,
    name: 'general',
    parentId: 1,
    position: 0,
  ),
  Channel(
    id: 11,
    kind: ChannelKind.voice,
    name: 'Lounge',
    parentId: 1,
    position: 0,
  ),
  Channel(
    id: 12,
    kind: ChannelKind.text,
    name: 'random',
    parentId: 1,
    position: 1,
  ),
  Channel(
    id: 20,
    kind: ChannelKind.announcement,
    name: 'news',
    parentId: 2,
    position: 0,
  ),
  Channel(id: 30, kind: ChannelKind.text, name: 'lobby', position: 0),
  Channel(
    id: 31,
    kind: ChannelKind.text,
    name: 'orphan',
    parentId: 99,
    position: 1,
  ),
];

void main() {
  test('channels follow the sidebar order: loose ones, then categories', () {
    final tree = channelTree(channels);

    expect(tree.map((group) => group.category?.name), [null, 'Info', 'Text']);
    expect(tree.first.channels.map((c) => c.name), ['lobby', 'orphan']);
    expect(tree.last.channels.map((c) => c.name), [
      'general',
      'random',
      'Lounge',
    ]);
    expect(navigableChannels(channels).map((c) => c.id), [30, 31, 20, 10, 12]);
  });

  test('neighbors wrap around', () {
    final order = [for (final c in navigableChannels(channels)) c.id];

    expect(neighbor(order, 12, 1), 30);
    expect(neighbor(order, 30, -1), 12);
    expect(neighbor(order, 20, 1), 10);
    expect(neighbor(order, null, 1), 30);
    expect(neighbor(<int>[], null, 1), isNull);
  });

  test('the next unread channel skips read ones', () {
    final order = [30, 31, 20, 10, 12];
    bool unread(int id) => id == 10 || id == 31;

    expect(neighborWhere(order, 30, 1, unread), 31);
    expect(neighborWhere(order, 31, 1, unread), 10);
    expect(neighborWhere(order, 10, 1, unread), 31);
    expect(neighborWhere(order, 30, -1, unread), 10);
    expect(neighborWhere(order, 30, 1, (_) => false), isNull);
  });

  test('layouts change at 1180, 980 and 760 px', () {
    expect(ShellLayout.forWidth(1440), ShellLayout.wide);
    expect(ShellLayout.forWidth(1180), ShellLayout.wide);
    expect(ShellLayout.forWidth(1179), ShellLayout.medium);
    expect(ShellLayout.forWidth(979), ShellLayout.narrow);
    expect(ShellLayout.forWidth(759), ShellLayout.compact);
    expect(ShellLayout.wide.sidebarWidth, 304);
    expect(ShellLayout.narrow.sidebarWidth, 264);
    expect(ShellLayout.wide.membersInline, isTrue);
    expect(ShellLayout.medium.membersInline, isFalse);
    expect(ShellLayout.compact.sidebarInline, isFalse);
  });

  test('the last server and each server’s last channel are remembered', () {
    final store = MemoryKeyValueStore();
    ProviderContainer app() {
      final container = ProviderContainer(
        overrides: [keyValueStoreProvider.overrideWithValue(store)],
      );
      addTearDown(container.dispose);
      return container;
    }

    app().read(navigationProvider.notifier)
      ..openChannel('a:1', 10)
      ..openChannel('b:2', 20)
      ..openServer('a:1');
    final restored = app().read(navigationProvider);

    expect(restored.server, 'a:1');
    expect(restored.channels, {'a:1': 10, 'b:2': 20});
  });

  test('the member list toggle is remembered', () {
    final store = MemoryKeyValueStore();
    final first = ProviderContainer(
      overrides: [keyValueStoreProvider.overrideWithValue(store)],
    );
    addTearDown(first.dispose);
    final shownByDefault = first.read(memberPanelProvider);

    first.read(memberPanelProvider.notifier).toggle();
    final second = ProviderContainer(
      overrides: [keyValueStoreProvider.overrideWithValue(store)],
    );
    addTearDown(second.dispose);

    expect(shownByDefault, isTrue);
    expect(second.read(memberPanelProvider), isFalse);
  });
}
