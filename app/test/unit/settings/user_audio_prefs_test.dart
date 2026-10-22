import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/user_audio_prefs.dart';

void main() {
  test('the same identity is the same person on every server', () {
    const there = User(id: 7, displayName: 'Mira', fingerprint: 'ab12');
    const here = User(id: 3, displayName: 'Mira O.', fingerprint: 'ab12');

    expect(userAudioKey('a.example:7710', there), 'ab12');
    expect(userAudioKey('b.example:7710', here), 'ab12');
    // Without a fingerprint only this server's id can say who it is.
    expect(
      userAudioKey('a.example:7710', const User(id: 7, displayName: 'Mira')),
      'a.example:7710 7',
    );
  });

  test('choices are saved, defaults are dropped, damage is ignored', () {
    final store = MemoryKeyValueStore()
      ..write(
        userAudioPrefsKey,
        jsonEncode({
          'ab12': {'volume': 900, 'muted': true},
          'cd34': {'volume': 100},
          'ef56': 'not a choice',
        }),
      );
    final container = ProviderContainer(
      overrides: [keyValueStoreProvider.overrideWithValue(store)],
    );
    addTearDown(container.dispose);

    expect(container.read(userAudioPrefsProvider), {
      'ab12': const UserAudio(volume: 200, muted: true),
    });
    container
        .read(userAudioPrefsProvider.notifier)
        .update('ab12', (audio) => audio.copyWith(muted: false, volume: 100));
    container
        .read(userAudioPrefsProvider.notifier)
        .update('gh78', (audio) => audio.copyWith(soundboardMuted: true));

    expect(jsonDecode(store.read(userAudioPrefsKey)!), {
      'gh78': {'volume': 100, 'muted': false, 'soundboardMuted': true},
    });
  });
}
