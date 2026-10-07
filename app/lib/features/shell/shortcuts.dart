import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'package:opencord/ui/widgets/key_hint.dart';

/// Steps through text channels (Alt+↑/↓).
class ChannelStepIntent extends Intent {
  const ChannelStepIntent(this.delta);

  final int delta;
}

/// Steps through unread channels (Alt+Shift+↑/↓).
class UnreadStepIntent extends Intent {
  const UnreadStepIntent(this.delta);

  final int delta;
}

/// Steps through servers (Ctrl+Alt+↑/↓).
class ServerStepIntent extends Intent {
  const ServerStepIntent(this.delta);

  final int delta;
}

class ToggleMuteIntent extends Intent {
  const ToggleMuteIntent();
}

class ToggleDeafenIntent extends Intent {
  const ToggleDeafenIntent();
}

class ToggleMembersIntent extends Intent {
  const ToggleMembersIntent();
}

/// Escape with nothing else to close: leaves voice focus mode, or marks
/// the open channel read.
class MarkReadIntent extends Intent {
  const MarkReadIntent();
}

class QuickSwitcherIntent extends Intent {
  const QuickSwitcherIntent();
}

class UserSettingsIntent extends Intent {
  const UserSettingsIntent();
}

class FullscreenIntent extends Intent {
  const FullscreenIntent();
}

/// Ctrl+Shift+F12 in debug builds (plan §11).
class DebugMenuIntent extends Intent {
  const DebugMenuIntent();
}

/// The app-wide shortcuts of §7. Ctrl becomes ⌘ on macOS; Alt stays ⌥.
Map<ShortcutActivator, Intent> shellShortcuts(TargetPlatform platform) {
  SingleActivator primary(
    LogicalKeyboardKey key, {
    bool shift = false,
    bool alt = false,
  }) => appShortcut(key, platform, shift: shift, alt: alt);
  final mac = platform == TargetPlatform.macOS;
  return {
    primary(LogicalKeyboardKey.keyK): const QuickSwitcherIntent(),
    primary(LogicalKeyboardKey.comma): const UserSettingsIntent(),
    const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true):
        const ChannelStepIntent(-1),
    const SingleActivator(LogicalKeyboardKey.arrowDown, alt: true):
        const ChannelStepIntent(1),
    const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true, shift: true):
        const UnreadStepIntent(-1),
    const SingleActivator(LogicalKeyboardKey.arrowDown, alt: true, shift: true):
        const UnreadStepIntent(1),
    primary(LogicalKeyboardKey.arrowUp, alt: true): const ServerStepIntent(-1),
    primary(LogicalKeyboardKey.arrowDown, alt: true): const ServerStepIntent(1),
    primary(LogicalKeyboardKey.keyM, shift: true): const ToggleMuteIntent(),
    primary(LogicalKeyboardKey.keyD, shift: true): const ToggleDeafenIntent(),
    primary(LogicalKeyboardKey.keyU, shift: true): const ToggleMembersIntent(),
    const SingleActivator(LogicalKeyboardKey.escape): const MarkReadIntent(),
    if (mac)
      const SingleActivator(LogicalKeyboardKey.keyF, control: true, meta: true):
          const FullscreenIntent()
    else
      const SingleActivator(LogicalKeyboardKey.f11): const FullscreenIntent(),
    if (kDebugMode)
      primary(LogicalKeyboardKey.f12, shift: true): const DebugMenuIntent(),
  };
}
