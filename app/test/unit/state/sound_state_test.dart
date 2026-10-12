import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

// The soundboard in the app's state (Phase 2 V8): sounds per server, who is
// playing one now, and this device's soundboard choices.

const _server = 'opencord.example:7710';

Sound _sound(int id, String name) => Sound(
  id: id,
  name: name,
  sha256: 'hash$id',
  duration: const Duration(seconds: 1),
);

SoundPlay _play(int userId, Duration length) => SoundPlay(
  channelId: 5,
  userId: userId,
  soundRef: 'default:boing',
  name: 'Boing',
  duration: length,
  startedAt: DateTime(2026),
);

ProviderContainer _container([MemoryKeyValueStore? store]) {
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(store ?? MemoryKeyValueStore()),
    ],
  );
  addTearDown(container.dispose);
  return container;
}

void main() {
  test('sounds come, change and go', () {
    final container = _container();
    final sounds = container.read(soundsProvider(_server).notifier);

    sounds.apply(SoundUpserted(_server, _sound(1, 'Bruh')));
    sounds.apply(SoundUpserted(_server, _sound(2, 'Wow')));
    sounds.apply(SoundUpserted(_server, _sound(1, 'Bruh!')));
    sounds.apply(const SoundDeleted(_server, 2));

    expect(container.read(soundsProvider(_server)), {1: _sound(1, 'Bruh!')});
  });

  test(
    "a player's ring lasts as long as the sound, a new sound restarts it",
    () {
      fakeAsync((async) {
        final container = _container();
        final plays = container.read(soundPlaysProvider(_server).notifier);

        plays.played(_play(7, const Duration(seconds: 2)));
        async.elapse(const Duration(seconds: 1));
        final again = _play(7, const Duration(seconds: 2));
        plays.played(again);
        async.elapse(const Duration(milliseconds: 1500));
        expect(container.read(soundPlaysProvider(_server)), {7: again});

        async.elapse(const Duration(milliseconds: 600));
        expect(container.read(soundPlaysProvider(_server)), isEmpty);
      });
    },
  );

  test('soundboard choices and favourites are kept on this device', () {
    final store = MemoryKeyValueStore();
    final container = _container(store);
    final prefs = container.read(soundboardPrefsProvider.notifier);

    prefs.update((prefs) => prefs.copyWith(volume: 40, muted: true));
    prefs.toggleFavorite(favoriteKey('server:ab', serverKey: _server));
    prefs.toggleFavorite('default:boing');
    prefs.toggleFavorite('default:boing');

    final reopened = _container(store).read(soundboardPrefsProvider);
    expect(reopened.volume, 40);
    expect(reopened.muted, isTrue);
    expect(reopened.favorites, {'$_server server:ab'});
    expect(
      SoundboardPrefs.fromJson({'volume': 900, 'favorites': 'nope'}),
      const SoundboardPrefs(),
      reason: 'damaged values fall back',
    );
  });
}
