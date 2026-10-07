import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';

/// Decodes the JSON saved under [key], or null when it is missing or
/// damaged.
Object? _readJson(KeyValueStore store, String key) {
  final saved = store.read(key);
  if (saved == null) return null;
  try {
    return jsonDecode(saved);
  } on FormatException {
    return null;
  }
}

/// Notification choices (§4.1, §4.2): muted servers and channels. Muted
/// ones keep a grey unread count and do not add to the window title.
@immutable
class NotificationPrefs {
  const NotificationPrefs({
    this.mutedServers = const {},
    this.mutedChannels = const {},
  });

  final Set<String> mutedServers;
  final Map<String, Set<int>> mutedChannels;

  bool serverMuted(String server) => mutedServers.contains(server);

  bool channelMuted(String server, int channel) =>
      mutedChannels[server]?.contains(channel) ?? false;

  /// Muted directly or through its server.
  bool muted(String server, int channel) =>
      serverMuted(server) || channelMuted(server, channel);

  Map<String, Object?> toJson() => {
    'servers': mutedServers.toList(),
    'channels': {
      for (final MapEntry(:key, :value) in mutedChannels.entries)
        key: value.toList(),
    },
  };

  static NotificationPrefs? fromJson(Object? json) {
    if (json is! Map) return null;
    final servers = json['servers'];
    final channels = json['channels'];
    return NotificationPrefs(
      mutedServers: {if (servers is List) ...servers.whereType<String>()},
      mutedChannels: {
        if (channels is Map)
          for (final MapEntry(:key, :value) in channels.entries)
            if (key is String && value is List)
              key: value.whereType<int>().toSet(),
      },
    );
  }
}

/// Servers muted until the user decides otherwise (the mock's "Homelab").
final defaultMutedServersProvider = Provider<Set<String>>((ref) => const {});

const notificationPrefsKey = 'ui.notifications';

class NotificationPrefsNotifier extends Notifier<NotificationPrefs> {
  @override
  NotificationPrefs build() =>
      NotificationPrefs.fromJson(
        _readJson(ref.watch(keyValueStoreProvider), notificationPrefsKey),
      ) ??
      NotificationPrefs(mutedServers: ref.watch(defaultMutedServersProvider));

  void toggleServer(String server) {
    final servers = {...state.mutedServers};
    if (!servers.remove(server)) servers.add(server);
    _save(
      NotificationPrefs(
        mutedServers: servers,
        mutedChannels: state.mutedChannels,
      ),
    );
  }

  void toggleChannel(String server, int channel) {
    final channels = {...?state.mutedChannels[server]};
    if (!channels.remove(channel)) channels.add(channel);
    _save(
      NotificationPrefs(
        mutedServers: state.mutedServers,
        mutedChannels: {...state.mutedChannels, server: channels},
      ),
    );
  }

  void _save(NotificationPrefs prefs) {
    state = prefs;
    ref
        .read(keyValueStoreProvider)
        .write(notificationPrefsKey, jsonEncode(prefs.toJson()));
  }
}

final notificationPrefsProvider =
    NotifierProvider<NotificationPrefsNotifier, NotificationPrefs>(
      NotificationPrefsNotifier.new,
    );

String categoryKey(String server, int category) => '$server#$category';

const collapsedCategoriesKey = 'ui.collapsed';

/// Collapsed sidebar categories, remembered (§16).
class CollapsedCategoriesNotifier extends Notifier<Set<String>> {
  @override
  Set<String> build() {
    final json = _readJson(
      ref.watch(keyValueStoreProvider),
      collapsedCategoriesKey,
    );
    return json is List ? json.whereType<String>().toSet() : const {};
  }

  void toggle(String server, int category) {
    final collapsed = {...state};
    final key = categoryKey(server, category);
    if (!collapsed.remove(key)) collapsed.add(key);
    state = collapsed;
    ref
        .read(keyValueStoreProvider)
        .write(collapsedCategoriesKey, jsonEncode(collapsed.toList()));
  }
}

final collapsedCategoriesProvider =
    NotifierProvider<CollapsedCategoriesNotifier, Set<String>>(
      CollapsedCategoriesNotifier.new,
    );

const selfPresenceKey = 'ui.presence';

/// The presence the user chose for themselves (§4.2 user panel). The
/// repository is told when it changes and at startup.
class SelfPresenceNotifier extends Notifier<SelfPresence> {
  @override
  SelfPresence build() {
    final saved = ref.watch(keyValueStoreProvider).read(selfPresenceKey);
    return SelfPresence.values.where((p) => p.name == saved).firstOrNull ??
        SelfPresence.online;
  }

  void choose(SelfPresence presence) {
    state = presence;
    ref.read(keyValueStoreProvider).write(selfPresenceKey, presence.name);
    if (ref.exists(repositoryProvider)) {
      ref.read(repositoryProvider).updatePresence(presence);
    }
  }
}

final selfPresenceProvider =
    NotifierProvider<SelfPresenceNotifier, SelfPresence>(
      SelfPresenceNotifier.new,
    );

enum InputMode {
  voiceActivity('Voice activity'),
  pushToTalk('Push to talk');

  const InputMode(this.label);

  final String label;
}

/// Audio choices reachable from the quick audio menu (§4.2, §17.1). Voice
/// is mock-only in Phase 1; these are saved so Phase 2 starts from them.
@immutable
class AudioSettings {
  const AudioSettings({
    this.inputDevice = 'Default',
    this.outputDevice = 'Default',
    this.inputMode = InputMode.voiceActivity,
    this.inputVolume = 100,
    this.outputVolume = 100,
  });

  static const maxVolume = 200;

  final String inputDevice;
  final String outputDevice;
  final InputMode inputMode;

  /// Microphone gain, 0–200 %.
  final int inputVolume;

  /// Everything you hear, 0–200 %.
  final int outputVolume;

  AudioSettings copyWith({
    String? inputDevice,
    String? outputDevice,
    InputMode? inputMode,
    int? inputVolume,
    int? outputVolume,
  }) => AudioSettings(
    inputDevice: inputDevice ?? this.inputDevice,
    outputDevice: outputDevice ?? this.outputDevice,
    inputMode: inputMode ?? this.inputMode,
    inputVolume: (inputVolume ?? this.inputVolume).clamp(0, maxVolume),
    outputVolume: (outputVolume ?? this.outputVolume).clamp(0, maxVolume),
  );

  Map<String, Object?> toJson() => {
    'inputDevice': inputDevice,
    'outputDevice': outputDevice,
    'inputMode': inputMode.name,
    'inputVolume': inputVolume,
    'outputVolume': outputVolume,
  };

  static AudioSettings fromJson(Object? json) {
    const defaults = AudioSettings();
    if (json is! Map) return defaults;
    return defaults.copyWith(
      inputDevice: json['inputDevice'] is String
          ? json['inputDevice'] as String
          : null,
      outputDevice: json['outputDevice'] is String
          ? json['outputDevice'] as String
          : null,
      inputMode: InputMode.values
          .where((m) => m.name == json['inputMode'])
          .firstOrNull,
      inputVolume: json['inputVolume'] is int
          ? json['inputVolume'] as int
          : null,
      outputVolume: json['outputVolume'] is int
          ? json['outputVolume'] as int
          : null,
    );
  }
}

const audioSettingsKey = 'ui.audio';

class AudioSettingsNotifier extends Notifier<AudioSettings> {
  @override
  AudioSettings build() => AudioSettings.fromJson(
    _readJson(ref.watch(keyValueStoreProvider), audioSettingsKey),
  );

  void update(AudioSettings Function(AudioSettings audio) change) {
    state = change(state);
    ref
        .read(keyValueStoreProvider)
        .write(audioSettingsKey, jsonEncode(state.toJson()));
  }
}

final audioSettingsProvider =
    NotifierProvider<AudioSettingsNotifier, AudioSettings>(
      AudioSettingsNotifier.new,
    );
