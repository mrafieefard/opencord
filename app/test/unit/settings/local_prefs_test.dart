import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

ProviderContainer app(
  KeyValueStore store, {
  Set<String> mutedByDefault = const {},
}) {
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(store),
      defaultMutedServersProvider.overrideWithValue(mutedByDefault),
    ],
  );
  addTearDown(container.dispose);
  return container;
}

void main() {
  test('muted servers and channels are remembered', () {
    final store = MemoryKeyValueStore();

    app(store).read(notificationPrefsProvider.notifier)
      ..toggleServer('a:1')
      ..toggleChannel('b:2', 10);
    final prefs = app(store).read(notificationPrefsProvider);

    expect(prefs.serverMuted('a:1'), isTrue);
    expect(prefs.channelMuted('b:2', 10), isTrue);
    expect(prefs.channelMuted('b:2', 11), isFalse);
    expect(
      prefs.muted('a:1', 99),
      isTrue,
      reason: 'a muted server mutes its channels',
    );
  });

  test('servers muted by default apply only until the user chooses', () {
    final store = MemoryKeyValueStore();
    final first = app(store, mutedByDefault: {'h:1'});
    final mutedAtFirst = first
        .read(notificationPrefsProvider)
        .serverMuted('h:1');

    first.read(notificationPrefsProvider.notifier).toggleServer('h:1');
    final later = app(
      store,
      mutedByDefault: {'h:1'},
    ).read(notificationPrefsProvider);

    expect(mutedAtFirst, isTrue);
    expect(later.serverMuted('h:1'), isFalse);
  });

  test('collapsed categories are remembered per server', () {
    final store = MemoryKeyValueStore();

    app(store).read(collapsedCategoriesProvider.notifier).toggle('a:1', 5);
    final collapsed = app(store).read(collapsedCategoriesProvider);

    expect(collapsed.contains(categoryKey('a:1', 5)), isTrue);
    expect(collapsed.contains(categoryKey('b:2', 5)), isFalse);
  });

  test('the chosen presence and the audio choices are remembered', () {
    final store = MemoryKeyValueStore();
    final first = app(store);

    first.read(selfPresenceProvider.notifier).choose(SelfPresence.doNotDisturb);
    first
        .read(audioSettingsProvider.notifier)
        .update(
          (audio) =>
              audio.copyWith(inputVolume: 150, inputMode: InputMode.pushToTalk),
        );
    final later = app(store);

    expect(later.read(selfPresenceProvider), SelfPresence.doNotDisturb);
    expect(later.read(audioSettingsProvider).inputVolume, 150);
    expect(later.read(audioSettingsProvider).inputMode, InputMode.pushToTalk);
  });

  test('volumes stay within 0–200 %', () {
    const audio = AudioSettings();

    expect(audio.copyWith(inputVolume: 300).inputVolume, 200);
    expect(audio.copyWith(outputVolume: -5).outputVolume, 0);
  });
}
