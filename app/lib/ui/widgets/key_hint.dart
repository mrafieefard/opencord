import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// The app's primary modifier: ⌘ on macOS, Ctrl elsewhere (§7).
SingleActivator appShortcut(
  LogicalKeyboardKey key,
  TargetPlatform platform, {
  bool shift = false,
  bool alt = false,
}) {
  final mac = platform == TargetPlatform.macOS;
  return SingleActivator(key, control: !mac, meta: mac, shift: shift, alt: alt);
}

/// `Ctrl+Shift+M` on Linux and Windows, `⇧⌘M` on macOS. Shortcuts written
/// with Ctrl show ⌘ on macOS, where the app binds them to ⌘.
String shortcutLabel(
  SingleActivator shortcut,
  TargetPlatform platform, {
  String separator = '+',
}) {
  final key = _keyName(shortcut.trigger, platform);
  if (platform == TargetPlatform.macOS) {
    return [
      if (shortcut.alt) '⌥',
      if (shortcut.shift) '⇧',
      if (shortcut.control || shortcut.meta) '⌘',
      key,
    ].join();
  }
  return [
    if (shortcut.control) 'Ctrl',
    if (shortcut.meta) 'Super',
    if (shortcut.alt) 'Alt',
    if (shortcut.shift) 'Shift',
    key,
  ].join(separator);
}

String _keyName(LogicalKeyboardKey key, TargetPlatform platform) {
  final mac = platform == TargetPlatform.macOS;
  final names = {
    LogicalKeyboardKey.arrowUp: '↑',
    LogicalKeyboardKey.arrowDown: '↓',
    LogicalKeyboardKey.arrowLeft: '←',
    LogicalKeyboardKey.arrowRight: '→',
    LogicalKeyboardKey.enter: mac ? '↩' : 'Enter',
    LogicalKeyboardKey.escape: 'Esc',
    LogicalKeyboardKey.pageUp: mac ? '⇞' : 'PgUp',
    LogicalKeyboardKey.pageDown: mac ? '⇟' : 'PgDn',
    LogicalKeyboardKey.space: 'Space',
    LogicalKeyboardKey.tab: mac ? '⇥' : 'Tab',
    LogicalKeyboardKey.delete: mac ? '⌦' : 'Del',
    LogicalKeyboardKey.backspace: mac ? '⌫' : 'Backspace',
    LogicalKeyboardKey.comma: ',',
    LogicalKeyboardKey.period: '.',
    LogicalKeyboardKey.slash: '/',
  };
  return names[key] ?? key.keyLabel.toUpperCase();
}

/// A small chip showing a shortcut, like `Ctrl K` or `⌘K`.
class KeyHint extends StatelessWidget {
  const KeyHint(this.shortcut, {super.key});

  final SingleActivator shortcut;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(5),
        border: Border.all(color: colors.border),
      ),
      child: Text(
        shortcutLabel(shortcut, Theme.of(context).platform, separator: ' '),
        style: OcText.meta.copyWith(color: colors.textMuted),
      ),
    );
  }
}
