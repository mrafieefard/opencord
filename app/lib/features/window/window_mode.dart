import 'package:flutter/foundation.dart';

/// Who draws the window frame (§3.1).
enum WindowChrome {
  /// The app draws the title bar into its header row, with window controls
  /// in the platform's style.
  custom,

  /// No native window to draw on (tests, unsupported platforms): the
  /// platform draws whatever frame there is.
  system,
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
