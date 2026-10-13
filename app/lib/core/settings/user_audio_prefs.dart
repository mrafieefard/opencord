import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

/// How someone sounds on this device: the per-user voice menu's volume,
/// Mute and Mute soundboard (desktop UI plan §17.3).
@immutable
class UserAudio {
  const UserAudio({
    this.volume = 100,
    this.muted = false,
    this.soundboardMuted = false,
  });

  static const maxVolume = 200;

  /// 0–200 %.
  final int volume;

  /// Local mute: nobody else is affected.
  final bool muted;

  /// Their soundboard sounds are not played here.
  final bool soundboardMuted;

  bool get isDefault => volume == 100 && !muted && !soundboardMuted;

  UserAudio copyWith({int? volume, bool? muted, bool? soundboardMuted}) =>
      UserAudio(
        volume: volume ?? this.volume,
        muted: muted ?? this.muted,
        soundboardMuted: soundboardMuted ?? this.soundboardMuted,
      );

  Map<String, Object?> toJson() => {
    'volume': volume,
    'muted': muted,
    'soundboardMuted': soundboardMuted,
  };

  static UserAudio fromJson(Object? json) {
    if (json is! Map) return const UserAudio();
    final volume = json['volume'];
    return UserAudio(
      volume: volume is int ? volume.clamp(0, maxVolume) : 100,
      muted: json['muted'] == true,
      soundboardMuted: json['soundboardMuted'] == true,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is UserAudio &&
      other.volume == volume &&
      other.muted == muted &&
      other.soundboardMuted == soundboardMuted;

  @override
  int get hashCode => Object.hash(volume, muted, soundboardMuted);
}

/// Whom a [UserAudio] belongs to: their identity's fingerprint, so the
/// choice follows them to every server they are on and survives restarts
/// (desktop UI plan §13); someone without one (never on a real server) by
/// server and id.
String userAudioKey(String serverKey, User user) =>
    user.fingerprint.isNotEmpty ? user.fingerprint : '$serverKey ${user.id}';

const userAudioPrefsKey = 'ui.userAudio';

/// Everyone's [UserAudio] on this device, by [userAudioKey]; only choices
/// that differ from the default are kept.
class UserAudioNotifier extends Notifier<Map<String, UserAudio>> {
  @override
  Map<String, UserAudio> build() {
    final json = readJson(ref.watch(keyValueStoreProvider), userAudioPrefsKey);
    if (json is! Map) return const {};
    return {
      for (final MapEntry(:key, :value) in json.entries)
        if (key is String) key: UserAudio.fromJson(value),
    }..removeWhere((_, audio) => audio.isDefault);
  }

  void update(String key, UserAudio Function(UserAudio audio) change) {
    final audio = change(state[key] ?? const UserAudio());
    final next = {...state}..remove(key);
    if (!audio.isDefault) next[key] = audio;
    state = next;
    ref
        .read(keyValueStoreProvider)
        .write(
          userAudioPrefsKey,
          jsonEncode({
            for (final MapEntry(:key, :value) in state.entries)
              key: value.toJson(),
          }),
        );
  }
}

final userAudioPrefsProvider =
    NotifierProvider<UserAudioNotifier, Map<String, UserAudio>>(
      UserAudioNotifier.new,
    );
