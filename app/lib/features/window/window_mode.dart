import 'package:flutter/foundation.dart';

import 'package:opencord/core/settings/app_settings.dart';

/// Who draws the window frame (§3.1).
enum WindowChrome {
  /// The app draws the title bar into its header row, with window controls
  /// in the platform's style.
  custom,

  /// The platform's own title bar and decorations.
  system,

  /// No decorations and no controls: a tiling compositor manages the window
  /// and the headers are just headers.
  bare,
}

const _tiling = {'hyprland', 'sway', 'i3', 'river', 'niri', 'bspwm', 'dwm'};

/// Picks the window chrome. On Linux, Auto reads `XDG_CURRENT_DESKTOP`:
/// KDE keeps its own frame, tiling compositors get none, everything else
/// gets the custom header bar.
WindowChrome resolveWindowChrome({
  required TargetPlatform platform,
  required Map<String, String> environment,
  required WindowFramePreference preference,
}) {
  switch (preference) {
    case WindowFramePreference.custom:
      return WindowChrome.custom;
    case WindowFramePreference.system:
      return WindowChrome.system;
    case WindowFramePreference.auto:
      break;
  }
  if (platform != TargetPlatform.linux) return WindowChrome.custom;
  final desktops = (environment['XDG_CURRENT_DESKTOP'] ?? '')
      .toLowerCase()
      .split(':')
      .map((name) => name.trim())
      .toSet();
  if (desktops.any(_tiling.contains)) return WindowChrome.bare;
  if (desktops.contains('kde')) return WindowChrome.system;
  return WindowChrome.custom;
}

enum WindowButton { minimize, maximize, close }

/// Which window buttons go on which side, from GNOME's
/// `gtk-decoration-layout` (like `appmenu:close`).
@immutable
class ButtonLayout {
  const ButtonLayout({required this.left, required this.right});

  factory ButtonLayout.parse(String layout) {
    List<WindowButton> buttons(String part) => [
      for (final name in part.split(','))
        ...WindowButton.values.where((button) => button.name == name.trim()),
    ];
    final colon = layout.indexOf(':');
    if (colon == -1) {
      return ButtonLayout(left: buttons(layout), right: const []);
    }
    return ButtonLayout(
      left: buttons(layout.substring(0, colon)),
      right: buttons(layout.substring(colon + 1)),
    );
  }

  /// GNOME shows only Close, on the right.
  static const gnomeDefault = ButtonLayout(
    left: [],
    right: [WindowButton.close],
  );

  final List<WindowButton> left;
  final List<WindowButton> right;

  @override
  bool operator ==(Object other) =>
      other is ButtonLayout &&
      listEquals(other.left, left) &&
      listEquals(other.right, right);

  @override
  int get hashCode => Object.hash(Object.hashAll(left), Object.hashAll(right));

  @override
  String toString() => 'ButtonLayout($left : $right)';
}
