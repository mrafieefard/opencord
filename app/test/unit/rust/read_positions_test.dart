import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/rust/read_positions.dart';
import 'package:opencord/core/settings/key_value_store.dart';

void main() {
  test('where each channel was read up to is kept per server', () {
    final store = MemoryKeyValueStore();
    ReadPositions(store)
      ..save('a.example:7710', 3, 100)
      ..save('a.example:7710', 4, 200)
      ..save('b.example:7710', 3, 5);

    final again = ReadPositions(store);

    expect(again.of('a.example:7710', 3), 100);
    expect(again.of('a.example:7710', 4), 200);
    expect(again.of('b.example:7710', 3), 5);
    expect(again.of('b.example:7710', 4), isNull);
  });

  test('a position never moves backwards', () {
    final positions = ReadPositions(MemoryKeyValueStore())
      ..save('a:1', 3, 100)
      ..save('a:1', 3, 90);

    expect(positions.of('a:1', 3), 100);
  });

  test('forgetting a server drops its positions', () {
    final store = MemoryKeyValueStore();
    ReadPositions(store)
      ..save('a:1', 3, 100)
      ..forget('a:1');

    expect(ReadPositions(store).of('a:1', 3), isNull);
  });

  test('damaged data reads as nothing', () {
    final store = MemoryKeyValueStore()..write('read.a:1', 'not json');

    expect(ReadPositions(store).of('a:1', 3), isNull);
  });
}
