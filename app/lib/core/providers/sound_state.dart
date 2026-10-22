import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/repository/repository.dart';

/// A server's soundboard (Phase 2 V8), by sound id.
class SoundsNotifier extends Notifier<Map<int, Sound>> {
  SoundsNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<int, Sound> build() => const {};

  void apply(RepoEvent event) {
    switch (event) {
      case Ready(:final snapshot):
        state = snapshot.sounds;
      case SoundUpserted(:final sound):
        state = {...state, sound.id: sound};
      case SoundDeleted(:final soundId) when state.containsKey(soundId):
        state = {...state}..remove(soundId);
      default:
        break;
    }
  }
}

final soundsProvider =
    NotifierProvider.family<SoundsNotifier, Map<int, Sound>, String>(
      SoundsNotifier.new,
    );

/// Who is playing a sound on a server now, by user (§11.6): their
/// soundboard ring shows for the sound's length.
class SoundPlaysNotifier extends Notifier<Map<int, SoundPlay>> {
  SoundPlaysNotifier(this.serverKey);

  final String serverKey;
  final _ends = <int, Timer>{};

  @override
  Map<int, SoundPlay> build() {
    ref.onDispose(() {
      for (final timer in _ends.values) {
        timer.cancel();
      }
    });
    return const {};
  }

  void played(SoundPlay play) {
    state = {...state, play.userId: play};
    _ends[play.userId]?.cancel();
    _ends[play.userId] = Timer(play.duration, () {
      _ends.remove(play.userId);
      if (state[play.userId] == play) {
        state = {...state}..remove(play.userId);
      }
    });
  }
}

final soundPlaysProvider =
    NotifierProvider.family<SoundPlaysNotifier, Map<int, SoundPlay>, String>(
      SoundPlaysNotifier.new,
    );

/// When this device last played a sound, per server: the picker's
/// cooldown bar (§11.7).
class LastSoundNotifier extends Notifier<Map<String, DateTime>> {
  @override
  Map<String, DateTime> build() => const {};

  void played(String serverKey, DateTime at) =>
      state = {...state, serverKey: at};
}

final lastSoundProvider =
    NotifierProvider<LastSoundNotifier, Map<String, DateTime>>(
      LastSoundNotifier.new,
    );
