import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/user_audio_prefs.dart';

/// Someone on a server: the key of per-user providers.
typedef UserRef = ({String server, int user});

/// The [userAudioKey] for someone, or null while they are not known.
final userAudioKeyProvider = Provider.family<String?, UserRef>((ref, who) {
  final user = ref.watch(
    serverProvider(who.server).select((s) => s.data?.members[who.user]?.user),
  );
  return user == null ? null : userAudioKey(who.server, user);
});

/// How someone sounds on this device (desktop UI plan §17.3).
final userAudioProvider = Provider.family<UserAudio, UserRef>((ref, who) {
  final key = ref.watch(userAudioKeyProvider(who));
  if (key == null) return const UserAudio();
  return ref.watch(userAudioPrefsProvider.select((prefs) => prefs[key])) ??
      const UserAudio();
});

/// What each other person in this device's voice channel should sound like
/// here, by user id; empty out of voice.
final callUserAudioProvider =
    Provider<({String? serverKey, Map<int, UserAudio> users})>((ref) {
      final session = ref.watch(voiceSessionProvider);
      final (serverKey, channelId) = (session.serverKey, session.channelId);
      if (serverKey == null || channelId == null) {
        return (serverKey: null, users: const {});
      }
      final participants = ref.watch(
        voiceProvider(
          serverKey,
        ).select((voice) => voice[channelId] ?? const <VoiceParticipant>[]),
      );
      final data = ref.watch(serverProvider(serverKey)).data;
      final prefs = ref.watch(userAudioPrefsProvider);
      return (
        serverKey: serverKey,
        users: {
          for (final participant in participants)
            if (participant.userId != data?.self.id)
              participant.userId:
                  switch (data?.members[participant.userId]?.user) {
                    final user? => prefs[userAudioKey(serverKey, user)],
                    null => null,
                  } ??
                  const UserAudio(),
        },
      );
    });

/// Hands the per-user choices to voice media whenever someone joins this
/// device's channel or a choice changes. Someone arriving gets all three,
/// so a choice changed while they were elsewhere still reaches them.
void applyUserAudio(Ref ref, OpencordRepository repository) {
  var applied = <int, UserAudio>{};
  String? appliedServer;
  ref.listen(callUserAudioProvider, (_, call) {
    final serverKey = call.serverKey;
    if (serverKey != appliedServer) applied = {};
    appliedServer = serverKey;
    if (serverKey == null) return;
    for (final MapEntry(key: userId, value: audio) in call.users.entries) {
      final before = applied[userId];
      if (before == null || before.volume != audio.volume) {
        repository.setUserVolume(serverKey, userId, audio.volume);
      }
      if (before == null || before.muted != audio.muted) {
        repository.setUserLocalMute(serverKey, userId, audio.muted);
      }
      if (before == null || before.soundboardMuted != audio.soundboardMuted) {
        repository.setUserSoundboardMute(
          serverKey,
          userId,
          audio.soundboardMuted,
        );
      }
    }
    applied = call.users;
  }, fireImmediately: true);
}
