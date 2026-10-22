import 'package:flutter/foundation.dart';

import 'package:opencord/core/repository/repository.dart';

/// What the tray icon shows (§15).
@immutable
class TrayState {
  const TrayState({
    this.unread = 0,
    this.mentions = 0,
    this.muted = false,
    this.deafened = false,
    this.presence = SelfPresence.online,
    this.dark = true,
  });

  /// Unread messages in channels that are not muted, as in the title.
  final int unread;
  final int mentions;
  final bool muted;
  final bool deafened;
  final SelfPresence presence;

  /// The desktop is dark, so the monochrome icon is white.
  final bool dark;

  String get tooltip {
    if (unread == 0) return 'No unread messages';
    final messages = '$unread unread message${unread == 1 ? '' : 's'}';
    if (mentions == 0) return messages;
    return '$messages, $mentions mention${mentions == 1 ? '' : 's'}';
  }

  @override
  bool operator ==(Object other) =>
      other is TrayState &&
      other.unread == unread &&
      other.mentions == mentions &&
      other.muted == muted &&
      other.deafened == deafened &&
      other.presence == presence &&
      other.dark == dark;

  @override
  int get hashCode =>
      Object.hash(unread, mentions, muted, deafened, presence, dark);
}

/// What the tray asks the app to do.
@immutable
sealed class TrayAction {
  const TrayAction();
}

final class OpenWindow extends TrayAction {
  const OpenWindow();

  @override
  bool operator ==(Object other) => other is OpenWindow;

  @override
  int get hashCode => (OpenWindow).hashCode;
}

final class ToggleMute extends TrayAction {
  const ToggleMute();

  @override
  bool operator ==(Object other) => other is ToggleMute;

  @override
  int get hashCode => (ToggleMute).hashCode;
}

final class ToggleDeafen extends TrayAction {
  const ToggleDeafen();

  @override
  bool operator ==(Object other) => other is ToggleDeafen;

  @override
  int get hashCode => (ToggleDeafen).hashCode;
}

final class SetPresence extends TrayAction {
  const SetPresence(this.presence);

  final SelfPresence presence;

  @override
  bool operator ==(Object other) =>
      other is SetPresence && other.presence == presence;

  @override
  int get hashCode => presence.hashCode;
}

final class QuitApp extends TrayAction {
  const QuitApp();

  @override
  bool operator ==(Object other) => other is QuitApp;

  @override
  int get hashCode => (QuitApp).hashCode;
}

/// One row of the tray menu. Ids are stable, and 0 is the menu itself.
@immutable
class TrayMenuItem {
  const TrayMenuItem({
    required this.id,
    required this.label,
    this.action,
    this.checked,
    this.radio = false,
    this.children = const [],
  }) : separator = false;

  const TrayMenuItem.separator({required this.id})
    : label = '',
      action = null,
      checked = null,
      radio = false,
      children = const [],
      separator = true;

  final int id;
  final String label;
  final TrayAction? action;

  /// Shown with a check mark (or a radio dot) when not null.
  final bool? checked;
  final bool radio;
  final List<TrayMenuItem> children;
  final bool separator;
}

/// The tray menu of §15: Open, Mute, Deafen, Status ▸, Quit.
List<TrayMenuItem> trayMenu(TrayState state) => [
  const TrayMenuItem(id: 1, label: 'Open Opencord', action: OpenWindow()),
  const TrayMenuItem.separator(id: 2),
  TrayMenuItem(
    id: 3,
    label: 'Mute',
    action: const ToggleMute(),
    checked: state.muted,
  ),
  TrayMenuItem(
    id: 4,
    label: 'Deafen',
    action: const ToggleDeafen(),
    checked: state.deafened,
  ),
  TrayMenuItem(
    id: 5,
    label: 'Status',
    children: [
      for (final (index, presence) in SelfPresence.values.indexed)
        TrayMenuItem(
          id: 10 + index,
          label: presence.label,
          action: SetPresence(presence),
          checked: presence == state.presence,
          radio: true,
        ),
    ],
  ),
  const TrayMenuItem.separator(id: 6),
  const TrayMenuItem(id: 7, label: 'Quit Opencord', action: QuitApp()),
];
