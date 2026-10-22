import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

/// The voice sounds, in the groups User settings → Sounds switches.
enum CueGroup {
  joinLeave('Joining and leaving', {VoiceCue.join, VoiceCue.leave}),
  othersJoinLeave('Others joining and leaving', {
    VoiceCue.userJoin,
    VoiceCue.userLeave,
  }),
  mute('Mute and unmute', {VoiceCue.mute, VoiceCue.unmute}),
  deafen('Deafen and undeafen', {VoiceCue.deafen, VoiceCue.undeafen}),
  stream('Streams starting and ending', {
    VoiceCue.streamStart,
    VoiceCue.streamStop,
  });

  const CueGroup(this.label, this.cues);

  final String label;
  final Set<VoiceCue> cues;
}

const cuePrefsKey = 'ui.voiceCues';

/// The voice sound groups turned off; every one is on at first, as on
/// Discord.
class CuePrefsNotifier extends Notifier<Set<CueGroup>> {
  @override
  Set<CueGroup> build() {
    final json = readJson(ref.watch(keyValueStoreProvider), cuePrefsKey);
    return json is List
        ? {
            for (final name in json.whereType<String>())
              ...CueGroup.values.where((group) => group.name == name),
          }
        : const {};
  }

  void set(CueGroup group, {required bool on}) {
    state = on ? ({...state}..remove(group)) : {...state, group};
    ref
        .read(keyValueStoreProvider)
        .write(
          cuePrefsKey,
          jsonEncode([for (final group in state) group.name]..sort()),
        );
  }
}

final cuePrefsProvider = NotifierProvider<CuePrefsNotifier, Set<CueGroup>>(
  CuePrefsNotifier.new,
);

/// What the voice sounds follow: where this device is in voice, and who
/// else is there and live.
@immutable
class CueScene {
  const CueScene({
    this.serverKey,
    this.channelId,
    this.muted = false,
    this.deafened = false,
    this.others = const {},
    this.live = const {},
  });

  final String? serverKey;
  final int? channelId;
  final bool muted;
  final bool deafened;

  /// Everyone else in the channel.
  final Set<int> others;

  /// Who in the channel is live, this device included.
  final Set<int> live;

  @override
  bool operator ==(Object other) =>
      other is CueScene &&
      other.serverKey == serverKey &&
      other.channelId == channelId &&
      other.muted == muted &&
      other.deafened == deafened &&
      setEquals(other.others, others) &&
      setEquals(other.live, live);

  @override
  int get hashCode => Object.hash(
    serverKey,
    channelId,
    muted,
    deafened,
    Object.hashAllUnordered(others),
    Object.hashAllUnordered(live),
  );
}

final cueSceneProvider = Provider<CueScene>((ref) {
  final session = ref.watch(voiceSessionProvider);
  final server = session.serverKey;
  final channel = session.channelId;
  if (server == null || channel == null) return const CueScene();
  final self = ref.watch(
    serverProvider(server).select((state) => state.data?.self.id),
  );
  final participants = ref.watch(
    voiceProvider(server).select((voice) => voice[channel] ?? const []),
  );
  return CueScene(
    serverKey: server,
    channelId: channel,
    muted: session.muted,
    deafened: session.deafened,
    others: {
      for (final participant in participants)
        if (participant.userId != self) participant.userId,
    },
    live: {
      for (final participant in participants)
        if (participant.screensharing) participant.userId,
    },
  );
});

/// The sounds that going from [before] to [after] makes, in order.
List<VoiceCue> cuesBetween(CueScene before, CueScene after) {
  final wasIn = before.channelId != null;
  final isIn = after.channelId != null;
  if (!wasIn && !isIn) return const [];
  if (!isIn) return const [VoiceCue.leave];
  if (!wasIn ||
      before.channelId != after.channelId ||
      before.serverKey != after.serverKey) {
    return const [VoiceCue.join];
  }
  final cues = <VoiceCue>[
    if (after.deafened != before.deafened)
      after.deafened ? VoiceCue.deafen : VoiceCue.undeafen
    else if (after.muted != before.muted)
      after.muted ? VoiceCue.mute : VoiceCue.unmute,
  ];
  // Deafened, others' comings and goings are not heard either.
  if (!after.deafened) {
    if (after.others.difference(before.others).isNotEmpty) {
      cues.add(VoiceCue.userJoin);
    }
    if (before.others.difference(after.others).isNotEmpty) {
      cues.add(VoiceCue.userLeave);
    }
    if (after.live.difference(before.live).isNotEmpty) {
      cues.add(VoiceCue.streamStart);
    }
    if (before.live.difference(after.live).isNotEmpty) {
      cues.add(VoiceCue.streamStop);
    }
  }
  return cues;
}

/// Plays the voice sounds as the scene changes, leaving out the groups
/// turned off. Kept alive by the shell.
final voiceCuesProvider = Provider<void>((ref) {
  ref.listen(cueSceneProvider, (before, after) {
    if (before == null) return;
    final off = ref.read(cuePrefsProvider);
    final repository = ref.read(repositoryProvider);
    for (final cue in cuesBetween(before, after)) {
      if (off.any((group) => group.cues.contains(cue))) continue;
      repository.playCue(cue);
    }
  });
});
