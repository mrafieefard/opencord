import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/voice.dart';
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

const trustedLinkDomainsKey = 'ui.trustedLinkDomains';

/// Web domains whose links open without asking (§4.5).
class TrustedLinkDomainsNotifier extends Notifier<Set<String>> {
  @override
  Set<String> build() {
    final json = _readJson(
      ref.watch(keyValueStoreProvider),
      trustedLinkDomainsKey,
    );
    return json is List ? json.whereType<String>().toSet() : const {};
  }

  void trust(String domain) {
    final lower = domain.toLowerCase();
    if (state.contains(lower)) return;
    state = {...state, lower};
    ref
        .read(keyValueStoreProvider)
        .write(trustedLinkDomainsKey, jsonEncode(state.toList()));
  }

  void forget(String domain) {
    if (!state.contains(domain)) return;
    state = {...state}..remove(domain);
    ref
        .read(keyValueStoreProvider)
        .write(trustedLinkDomainsKey, jsonEncode(state.toList()));
  }
}

final trustedLinkDomainsProvider =
    NotifierProvider<TrustedLinkDomainsNotifier, Set<String>>(
      TrustedLinkDomainsNotifier.new,
    );

/// The quick reactions before the user has a history (§4.5).
const defaultReactions = ['👍', '❤️', '😂', '😮', '😢', '🙏', '🎉', '🔥'];

/// The [count] emoji used most, topped up with [defaultReactions]. Ties go
/// to the default order, so the pill does not reshuffle at random.
List<String> frequentEmoji(Map<String, int> usage, {int count = 8}) {
  int place(String emoji) {
    final index = defaultReactions.indexOf(emoji);
    return index == -1 ? defaultReactions.length : index;
  }

  final used = usage.keys.toList()
    ..sort((a, b) {
      final byUse = usage[b]!.compareTo(usage[a]!);
      if (byUse != 0) return byUse;
      final byDefault = place(a).compareTo(place(b));
      return byDefault != 0 ? byDefault : a.compareTo(b);
    });
  return [
    ...used,
    ...defaultReactions.where((emoji) => !usage.containsKey(emoji)),
  ].take(count).toList();
}

const emojiUsageKey = 'ui.emojiUsage';

/// How often each emoji was picked, for "frequently used" in the picker
/// and the quick reaction pill.
class EmojiUsageNotifier extends Notifier<Map<String, int>> {
  @override
  Map<String, int> build() {
    final json = _readJson(ref.watch(keyValueStoreProvider), emojiUsageKey);
    return {
      if (json is Map)
        for (final MapEntry(:key, :value) in json.entries)
          if (key is String && value is int && value > 0) key: value,
    };
  }

  void use(String emoji) {
    state = {...state, emoji: (state[emoji] ?? 0) + 1};
    ref.read(keyValueStoreProvider).write(emojiUsageKey, jsonEncode(state));
  }
}

final emojiUsageProvider =
    NotifierProvider<EmojiUsageNotifier, Map<String, int>>(
      EmojiUsageNotifier.new,
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

/// Audio choices reachable from the quick audio menu (§4.2, §17.1), saved
/// on this device; voice media follows them (Phase 2 plan §7.12).
@immutable
class AudioSettings {
  const AudioSettings({
    this.inputDevice,
    this.inputDeviceName,
    this.outputDevice,
    this.outputDeviceName,
    this.inputMode = InputMode.voiceActivity,
    this.pushToTalkRelease = defaultRelease,
    this.automaticSensitivity = true,
    this.sensitivityDbfs = defaultSensitivityDbfs,
    this.echoCancellation = true,
    this.noiseSuppression,
    this.automaticGain = true,
    this.inputVolume = 100,
    this.outputVolume = 100,
  });

  static const maxVolume = 200;

  /// Push-to-talk's release delay: 200 ms unless chosen, up to 2 s (plan
  /// §7.4).
  static const defaultRelease = Duration(milliseconds: 200);
  static const maxRelease = Duration(seconds: 2);

  /// Manual sensitivity's level, from -100 to 0 dBFS.
  static const defaultSensitivityDbfs = -45.0;
  static const minSensitivityDbfs = -100.0;

  /// A device id; null follows the system's default.
  final String? inputDevice;

  /// The chosen device's name, shown while it is unplugged.
  final String? inputDeviceName;
  final String? outputDevice;
  final String? outputDeviceName;
  final InputMode inputMode;
  final Duration pushToTalkRelease;
  final bool automaticSensitivity;
  final double sensitivityDbfs;
  final bool echoCancellation;

  /// Null until the first run's benchmark chooses High or Standard (plan
  /// §7.3).
  final NoiseSuppression? noiseSuppression;
  final bool automaticGain;

  /// Microphone gain, 0–200 %.
  final int inputVolume;

  /// Everything you hear, 0–200 %.
  final int outputVolume;

  /// The microphone to use; null for the system's default.
  AudioSettings withInput(AudioDevice? device) =>
      _copy(inputDevice: () => device?.id, inputDeviceName: () => device?.name);

  /// The speaker to use; null for the system's default.
  AudioSettings withOutput(AudioDevice? device) => _copy(
    outputDevice: () => device?.id,
    outputDeviceName: () => device?.name,
  );

  AudioSettings copyWith({
    InputMode? inputMode,
    Duration? pushToTalkRelease,
    bool? automaticSensitivity,
    double? sensitivityDbfs,
    bool? echoCancellation,
    NoiseSuppression? noiseSuppression,
    bool? automaticGain,
    int? inputVolume,
    int? outputVolume,
  }) => _copy(
    inputMode: inputMode,
    pushToTalkRelease: pushToTalkRelease,
    automaticSensitivity: automaticSensitivity,
    sensitivityDbfs: sensitivityDbfs,
    echoCancellation: echoCancellation,
    noiseSuppression: noiseSuppression,
    automaticGain: automaticGain,
    inputVolume: inputVolume,
    outputVolume: outputVolume,
  );

  AudioSettings _copy({
    String? Function()? inputDevice,
    String? Function()? inputDeviceName,
    String? Function()? outputDevice,
    String? Function()? outputDeviceName,
    InputMode? inputMode,
    Duration? pushToTalkRelease,
    bool? automaticSensitivity,
    double? sensitivityDbfs,
    bool? echoCancellation,
    NoiseSuppression? noiseSuppression,
    bool? automaticGain,
    int? inputVolume,
    int? outputVolume,
  }) {
    final release = pushToTalkRelease ?? this.pushToTalkRelease;
    return AudioSettings(
      inputDevice: inputDevice == null ? this.inputDevice : inputDevice(),
      inputDeviceName: inputDeviceName == null
          ? this.inputDeviceName
          : inputDeviceName(),
      outputDevice: outputDevice == null ? this.outputDevice : outputDevice(),
      outputDeviceName: outputDeviceName == null
          ? this.outputDeviceName
          : outputDeviceName(),
      inputMode: inputMode ?? this.inputMode,
      pushToTalkRelease: release < Duration.zero
          ? Duration.zero
          : (release > maxRelease ? maxRelease : release),
      automaticSensitivity: automaticSensitivity ?? this.automaticSensitivity,
      sensitivityDbfs: (sensitivityDbfs ?? this.sensitivityDbfs)
          .clamp(minSensitivityDbfs, 0)
          .toDouble(),
      echoCancellation: echoCancellation ?? this.echoCancellation,
      noiseSuppression: noiseSuppression ?? this.noiseSuppression,
      automaticGain: automaticGain ?? this.automaticGain,
      inputVolume: (inputVolume ?? this.inputVolume).clamp(0, maxVolume),
      outputVolume: (outputVolume ?? this.outputVolume).clamp(0, maxVolume),
    );
  }

  /// What voice media takes.
  AudioConfig get config => AudioConfig(
    inputDevice: inputDevice,
    outputDevice: outputDevice,
    pushToTalk: inputMode == InputMode.pushToTalk,
    pushToTalkRelease: pushToTalkRelease,
    automaticSensitivity: automaticSensitivity,
    sensitivityDbfs: sensitivityDbfs,
    echoCancellation: echoCancellation,
    noiseSuppression: noiseSuppression ?? NoiseSuppression.standard,
    automaticGain: automaticGain,
    inputVolume: inputVolume,
    outputVolume: outputVolume,
  );

  Map<String, Object?> toJson() => {
    'inputDevice': inputDevice,
    'inputDeviceName': inputDeviceName,
    'outputDevice': outputDevice,
    'outputDeviceName': outputDeviceName,
    'inputMode': inputMode.name,
    'pushToTalkReleaseMs': pushToTalkRelease.inMilliseconds,
    'automaticSensitivity': automaticSensitivity,
    'sensitivityDbfs': sensitivityDbfs,
    'echoCancellation': echoCancellation,
    'noiseSuppression': noiseSuppression?.name,
    'automaticGain': automaticGain,
    'inputVolume': inputVolume,
    'outputVolume': outputVolume,
  };

  static AudioSettings fromJson(Object? json) {
    const defaults = AudioSettings();
    if (json is! Map) return defaults;
    // Ids look like "host:device"; Phase 1 saved stand-in names instead,
    // which mean the default device now.
    String? id(Object? value) =>
        value is String && value.contains(':') ? value : null;
    String? name(Object? value) => value is String ? value : null;
    bool? flag(Object? value) => value is bool ? value : null;
    final inputDevice = id(json['inputDevice']);
    final outputDevice = id(json['outputDevice']);
    final releaseMs = json['pushToTalkReleaseMs'];
    final sensitivity = json['sensitivityDbfs'];
    return AudioSettings(
      inputDevice: inputDevice,
      inputDeviceName: inputDevice == null
          ? null
          : name(json['inputDeviceName']),
      outputDevice: outputDevice,
      outputDeviceName: outputDevice == null
          ? null
          : name(json['outputDeviceName']),
    ).copyWith(
      inputMode: InputMode.values
          .where((m) => m.name == json['inputMode'])
          .firstOrNull,
      pushToTalkRelease: releaseMs is int
          ? Duration(milliseconds: releaseMs)
          : null,
      automaticSensitivity: flag(json['automaticSensitivity']),
      sensitivityDbfs: sensitivity is num ? sensitivity.toDouble() : null,
      echoCancellation: flag(json['echoCancellation']),
      noiseSuppression: NoiseSuppression.values
          .where((mode) => mode.name == json['noiseSuppression'])
          .firstOrNull,
      automaticGain: flag(json['automaticGain']),
      inputVolume: json['inputVolume'] is int
          ? json['inputVolume'] as int
          : null,
      outputVolume: json['outputVolume'] is int
          ? json['outputVolume'] as int
          : null,
    );
  }
}

/// The hotkeys chosen on this device (Phase 2 plan §7.13): per action, a
/// key and its modifiers as the XDG shortcuts specification writes them,
/// such as `CTRL+SHIFT+m` or `grave`. None until chosen.
@immutable
class HotkeyBindings {
  const HotkeyBindings([this.bindings = const {}]);

  final Map<HotkeyAction, String> bindings;

  Map<String, Object?> toJson() => {
    for (final MapEntry(:key, :value) in bindings.entries) key.name: value,
  };

  static HotkeyBindings fromJson(Object? json) {
    if (json is! Map) return const HotkeyBindings();
    return HotkeyBindings({
      for (final action in HotkeyAction.values)
        if (json[action.name] case final String accelerator)
          action: accelerator,
    });
  }

  @override
  bool operator ==(Object other) =>
      other is HotkeyBindings && mapEquals(other.bindings, bindings);

  @override
  int get hashCode => Object.hashAllUnordered(
    bindings.entries.map((entry) => Object.hash(entry.key, entry.value)),
  );
}

const hotkeyBindingsKey = 'ui.hotkeys';

class HotkeyBindingsNotifier extends Notifier<HotkeyBindings> {
  @override
  HotkeyBindings build() => HotkeyBindings.fromJson(
    _readJson(ref.watch(keyValueStoreProvider), hotkeyBindingsKey),
  );

  /// Binds [action] to [accelerator], or unbinds it with null.
  void bind(HotkeyAction action, String? accelerator) {
    final bindings = {...state.bindings}..remove(action);
    if (accelerator != null) bindings[action] = accelerator;
    state = HotkeyBindings(bindings);
    ref
        .read(keyValueStoreProvider)
        .write(hotkeyBindingsKey, jsonEncode(state.toJson()));
  }
}

final hotkeyBindingsProvider =
    NotifierProvider<HotkeyBindingsNotifier, HotkeyBindings>(
      HotkeyBindingsNotifier.new,
    );

/// What a device picker offers: the system's default first, then each
/// device.
List<(AudioDevice?, String)> deviceChoices(List<AudioDevice> devices) => [
  (null, 'Default'),
  for (final device in devices) (device, device.name),
];

/// The chosen device's name: from the list, or as saved while it is
/// unplugged.
String deviceLabel(String? id, String? savedName, List<AudioDevice> devices) {
  if (id == null) return 'Default';
  return devices.where((device) => device.id == id).firstOrNull?.name ??
      savedName ??
      id;
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
