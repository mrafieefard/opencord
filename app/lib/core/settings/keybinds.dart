import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/hotkeys.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

/// One row of User settings → Keybinds, like Discord's: an action and the
/// keys that do it ([keys] as [Accelerator] writes them; empty until
/// recorded). [id] tells rows apart while they are edited.
@immutable
class Keybind {
  const Keybind({required this.id, required this.action, this.keys = ''});

  final int id;
  final HotkeyAction action;
  final String keys;

  Keybind copyWith({HotkeyAction? action, String? keys}) =>
      Keybind(id: id, action: action ?? this.action, keys: keys ?? this.keys);

  Map<String, Object?> toJson() => {'action': action.name, 'keys': keys};

  @override
  bool operator ==(Object other) =>
      other is Keybind &&
      other.id == id &&
      other.action == action &&
      other.keys == keys;

  @override
  int get hashCode => Object.hash(id, action, keys);
}

/// The keybinds chosen on this device (Phase 2 plan §7.13), in the order
/// they were added. None until added: Discord binds none either.
@immutable
class Keybinds {
  const Keybinds([this.list = const []]);

  final List<Keybind> list;

  /// What the system and the app watch for: every recorded keybind a
  /// hotkey can use, each once.
  List<HotkeyBinding> get bound {
    final seen = <HotkeyBinding>{};
    return [
      for (final keybind in list)
        if (Accelerator.parse(keybind.keys) != null)
          if (seen.add((action: keybind.action, keys: keybind.keys)))
            (action: keybind.action, keys: keybind.keys),
    ];
  }

  /// The keybinds of [action], in order.
  List<Keybind> of(HotkeyAction action) => [
    for (final keybind in list)
      if (keybind.action == action) keybind,
  ];

  /// Other keybinds with the same keys as [keybind]: one key press does
  /// all of them.
  List<Keybind> sharingKeys(Keybind keybind) => [
    for (final other in list)
      if (other.id != keybind.id &&
          keybind.keys.isNotEmpty &&
          other.keys == keybind.keys)
        other,
  ];

  /// With a new row for [action], with [keys] when known; and its id.
  (Keybinds, int) added(HotkeyAction action, {String keys = ''}) {
    final id = list.fold(-1, (most, row) => row.id > most ? row.id : most) + 1;
    return (
      Keybinds(
        List.unmodifiable([
          ...list,
          Keybind(id: id, action: action, keys: keys),
        ]),
      ),
      id,
    );
  }

  Keybinds changed(int id, Keybind Function(Keybind keybind) edit) => Keybinds(
    List.unmodifiable([
      for (final keybind in list) keybind.id == id ? edit(keybind) : keybind,
    ]),
  );

  Keybinds removed(int id) => Keybinds(
    List.unmodifiable([
      for (final keybind in list)
        if (keybind.id != id) keybind,
    ]),
  );

  List<Object?> toJson() => [for (final keybind in list) keybind.toJson()];

  /// A list of rows; or the earlier one-key-per-action map.
  static Keybinds fromJson(Object? json) {
    final rows = switch (json) {
      final List<Object?> list => [
        for (final row in list)
          if (row case {
            'action': final String action,
            'keys': final String keys,
          })
            (action, keys),
      ],
      final Map<Object?, Object?> map => [
        for (final MapEntry(:key, :value) in map.entries)
          if ((key, value) case (final String action, final String keys))
            (action, keys),
      ],
      _ => const <(String, String)>[],
    };
    final actions = {
      for (final action in HotkeyAction.values) action.name: action,
    };
    var id = 0;
    return Keybinds([
      for (final (name, keys) in rows)
        if (actions[name] case final action?)
          Keybind(id: id++, action: action, keys: keys),
    ]);
  }

  @override
  bool operator ==(Object other) =>
      other is Keybinds && listEquals(other.list, list);

  @override
  int get hashCode => Object.hashAll(list);
}

const keybindsKey = 'ui.hotkeys';

class KeybindsNotifier extends Notifier<Keybinds> {
  @override
  Keybinds build() => Keybinds.fromJson(
    readJson(ref.watch(keyValueStoreProvider), keybindsKey),
  );

  /// Saves [keybinds] as they are: what the settings' save bar applies.
  void replace(Keybinds keybinds) {
    state = keybinds;
    ref
        .read(keyValueStoreProvider)
        .write(keybindsKey, jsonEncode(state.toJson()));
  }

  /// A new row for [action], with [keys] when known; its id.
  int add(HotkeyAction action, {String keys = ''}) {
    final (keybinds, id) = state.added(action, keys: keys);
    replace(keybinds);
    return id;
  }

  void change(int id, Keybind Function(Keybind keybind) edit) =>
      replace(state.changed(id, edit));

  void remove(int id) => replace(state.removed(id));
}

final keybindsProvider = NotifierProvider<KeybindsNotifier, Keybinds>(
  KeybindsNotifier.new,
);

/// Whether a keybind is being recorded: keys pressed then are for the
/// recorder, not for the hotkeys already bound.
class RecordingKeybindNotifier extends Notifier<bool> {
  @override
  bool build() => false;

  void set(bool recording) => state = recording;
}

final recordingKeybindProvider =
    NotifierProvider<RecordingKeybindNotifier, bool>(
      RecordingKeybindNotifier.new,
    );
