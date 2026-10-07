import 'dart:async';
import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/window/native_window.dart';

/// Starting Opencord when the user logs in (§8.1, §15).
abstract interface class LoginItem {
  Future<void> setEnabled(bool enabled);
}

/// Where there is nothing to set: tests.
class NoLoginItem implements LoginItem {
  const NoLoginItem();

  @override
  Future<void> setEnabled(bool enabled) async {}
}

/// Linux: an XDG autostart entry, which desktops (and systemd's autostart
/// generator) start at login.
class XdgAutostart implements LoginItem {
  XdgAutostart({required this.configHome, required this.executable});

  final Directory configHome;
  final String executable;

  File get _entry =>
      File('${configHome.path}/autostart/dev.opencord.opencord.desktop');

  @override
  Future<void> setEnabled(bool enabled) async {
    final entry = _entry;
    if (!enabled) {
      if (entry.existsSync()) await entry.delete();
      return;
    }
    await entry.parent.create(recursive: true);
    await entry.writeAsString(
      [
        '[Desktop Entry]',
        'Type=Application',
        'Name=Opencord',
        'Exec=${_quote(executable)} $autostartArg',
        'Icon=dev.opencord.opencord',
        'Terminal=false',
        'X-GNOME-Autostart-enabled=true',
        '',
      ].join('\n'),
    );
  }

  /// A desktop entry's quoted argument: `"`, `` ` ``, `$` and `\` escaped.
  static String _quote(String value) =>
      '"${value.replaceAllMapped(RegExp(r'["`$\\]'), (m) => '\\${m[0]}')}"';
}

/// What a start at login is given, so Start minimized applies only then.
const autostartArg = '--autostart';

/// `$XDG_CONFIG_HOME`, or `~/.config` when it is not set.
Directory xdgConfigHome(Map<String, String> environment) {
  final config = environment['XDG_CONFIG_HOME'];
  if (config != null && config.isNotEmpty) return Directory(config);
  return Directory('${environment['HOME'] ?? ''}/.config');
}

/// Windows and macOS: the runner sets the Run key or the login item.
class RunnerLoginItem implements LoginItem {
  const RunnerLoginItem(this.window);

  final NativeWindow window;

  @override
  Future<void> setEnabled(bool enabled) => window.setLaunchAtLogin(enabled);
}

/// Overridden in `main` for each desktop.
final loginItemProvider = Provider<LoginItem>((ref) => const NoLoginItem());

/// Keeps the login item matching "Launch at login", from the start.
final loginItemBindingProvider = Provider<void>((ref) {
  final item = ref.watch(loginItemProvider);
  ref.listen(
    appSettingsProvider.select((settings) => settings.launchAtLogin),
    (_, enabled) => unawaited(item.setEnabled(enabled)),
    fireImmediately: true,
  );
});
