import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/chat_scroll.dart';

const _general = (server: 'opencord.example:7710', channel: 10004);
const _help = (server: 'opencord.example:7710', channel: 10007);

void main() {
  test('where a channel was left survives a restart (§16)', () {
    fakeAsync((async) {
      final store = MemoryKeyValueStore();
      ScrollMemory(
        store,
      ).save(_general, const SavedScroll(messageId: 42, fromTop: 120.5));
      async.elapse(const Duration(seconds: 2));

      final read = ScrollMemory(store).read(_general);

      expect(read?.messageId, 42);
      expect(read?.fromTop, 120.5);
    });
  });

  test('nothing is written until scrolling settles', () {
    fakeAsync((async) {
      final store = MemoryKeyValueStore();
      final memory = ScrollMemory(store);
      for (var step = 0; step < 10; step++) {
        memory.save(_general, SavedScroll(messageId: 42, fromTop: step * 10));
        async.elapse(const Duration(milliseconds: 100));
      }

      expect(store.read(ScrollMemory.storeKey), isNull);
      async.elapse(const Duration(seconds: 2));
      expect(ScrollMemory(store).read(_general)?.fromTop, 90);
    });
  });

  test('reaching the newest message forgets the place', () {
    fakeAsync((async) {
      final store = MemoryKeyValueStore();
      final memory = ScrollMemory(store)
        ..save(_general, const SavedScroll(messageId: 1, fromTop: 0))
        ..save(_help, const SavedScroll(messageId: 2, fromTop: 0));
      async.elapse(const Duration(seconds: 2));

      memory.save(_general, null);
      memory.flush();

      final restarted = ScrollMemory(store);
      expect(restarted.read(_general), isNull);
      expect(restarted.read(_help)?.messageId, 2);
    });
  });

  test('only the most recent channels are kept', () {
    final store = MemoryKeyValueStore();
    final memory = ScrollMemory(store);
    for (var channel = 0; channel <= ScrollMemory.limit; channel++) {
      memory.save((
        server: 's:1',
        channel: channel,
      ), SavedScroll(messageId: channel, fromTop: 0));
    }
    memory.flush();

    final restarted = ScrollMemory(store);
    expect(restarted.read((server: 's:1', channel: 0)), isNull);
    expect(
      restarted.read((server: 's:1', channel: ScrollMemory.limit))?.messageId,
      ScrollMemory.limit,
    );
  });

  test('damaged data is ignored', () {
    final store = MemoryKeyValueStore()
      ..write(ScrollMemory.storeKey, '{"s:1|3": "x", "broken');

    expect(ScrollMemory(store).read((server: 's:1', channel: 3)), isNull);
  });
}
