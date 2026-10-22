import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

const voiceViewPrefsKey = 'voice.view';

/// How the voice view is laid out (Phase 2 §12), remembered: the channel's
/// chat beside the tiles, and tiles only for people with video.
@immutable
class VoiceViewPrefs {
  const VoiceViewPrefs({this.chatOpen = false, this.hideNonVideo = false});

  final bool chatOpen;
  final bool hideNonVideo;

  VoiceViewPrefs copyWith({bool? chatOpen, bool? hideNonVideo}) =>
      VoiceViewPrefs(
        chatOpen: chatOpen ?? this.chatOpen,
        hideNonVideo: hideNonVideo ?? this.hideNonVideo,
      );

  Map<String, Object> toJson() => {
    'chatOpen': chatOpen,
    'hideNonVideo': hideNonVideo,
  };

  static VoiceViewPrefs fromJson(Object? json) {
    if (json is! Map) return const VoiceViewPrefs();
    return VoiceViewPrefs(
      chatOpen: json['chatOpen'] == true,
      hideNonVideo: json['hideNonVideo'] == true,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is VoiceViewPrefs &&
      other.chatOpen == chatOpen &&
      other.hideNonVideo == hideNonVideo;

  @override
  int get hashCode => Object.hash(chatOpen, hideNonVideo);
}

class VoiceViewPrefsNotifier extends Notifier<VoiceViewPrefs> {
  @override
  VoiceViewPrefs build() => VoiceViewPrefs.fromJson(
    readJson(ref.watch(keyValueStoreProvider), voiceViewPrefsKey),
  );

  void toggleChat() => _save(state.copyWith(chatOpen: !state.chatOpen));

  void toggleHideNonVideo() =>
      _save(state.copyWith(hideNonVideo: !state.hideNonVideo));

  void _save(VoiceViewPrefs prefs) {
    state = prefs;
    ref
        .read(keyValueStoreProvider)
        .write(voiceViewPrefsKey, jsonEncode(prefs.toJson()));
  }
}

final voiceViewPrefsProvider =
    NotifierProvider<VoiceViewPrefsNotifier, VoiceViewPrefs>(
      VoiceViewPrefsNotifier.new,
    );
