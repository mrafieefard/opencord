import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Which builds the launcher keeps the app on (docs/releasing.md).
enum ReleaseTrack {
  stable,
  canary,
  beta;

  static ReleaseTrack? parse(Object? name) => switch (name) {
    'stable' => stable,
    'canary' => canary,
    'beta' => beta,
    _ => null,
  };
}

/// The install the launcher made (docs/updater.md), when this app runs from
/// one: `<root>/app-<version>/opencord/opencord(.exe)`, or on macOS
/// `<root>/app-<version>/Opencord.app/Contents/MacOS/Opencord`. A build run
/// from an archive or from source has none and doesn't update itself.
class Installation {
  const Installation({
    required this.root,
    required this.version,
    required this.launcher,
  });

  /// The install folder, holding `update.json`.
  final Directory root;

  /// The installed version, from the folder's name.
  final String version;

  /// The launcher, which a restart goes through to update. Null when the
  /// app was started some other way (on macOS, from a link).
  final String? launcher;

  File get _settings =>
      File('${root.path}${Platform.pathSeparator}update.json');

  /// Finds the install around [executable]. [environment] carries what the
  /// launcher passed: `OPENCORD_LAUNCHER`.
  static Installation? find(
    String executable,
    Map<String, String> environment, {
    bool macOS = false,
  }) {
    final separators = RegExp(r'[/\\]');
    final parts = executable.split(separators);
    // From the executable up to `app-<version>`.
    final depth = macOS ? 5 : 3;
    if (parts.length <= depth) return null;
    final versionDir = parts[parts.length - depth];
    if (!versionDir.startsWith('app-') || versionDir.length == 4) return null;
    final rootParts = parts.sublist(0, parts.length - depth);
    final root = rootParts.join(Platform.pathSeparator);
    final launcher = environment['OPENCORD_LAUNCHER'];
    return Installation(
      root: Directory(root.isEmpty ? Platform.pathSeparator : root),
      version: versionDir.substring(4),
      launcher: launcher == null || launcher.isEmpty ? null : launcher,
    );
  }

  /// The track in `update.json`. Without one, the launcher uses the track it
  /// was built for, which [fallback] stands for.
  Future<ReleaseTrack> readTrack({
    ReleaseTrack fallback = ReleaseTrack.stable,
  }) async => ReleaseTrack.parse((await _read())['track']) ?? fallback;

  /// Sets the track, keeping what else `update.json` holds. The launcher
  /// reads it the next time it starts.
  Future<void> writeTrack(ReleaseTrack track) async {
    final settings = await _read();
    settings['track'] = track.name;
    final partial = File('${_settings.path}.partial');
    await partial.writeAsString(
      const JsonEncoder.withIndent('  ').convert(settings),
    );
    await partial.rename(_settings.path);
  }

  Future<Map<String, Object?>> _read() async {
    try {
      final decoded = jsonDecode(await _settings.readAsString());
      if (decoded is Map<String, Object?>) return {...decoded};
    } on FileSystemException {
      // Not written yet.
    } on FormatException {
      // Broken: rewritten whole.
    }
    return {};
  }

  /// Quits, having the launcher start the app again once this one is gone,
  /// with whatever update is waiting.
  Future<void> restart() async {
    final launcher = this.launcher;
    if (launcher == null) return;
    const macOSBundle = '.app/Contents/MacOS/';
    final inBundle = launcher.indexOf(macOSBundle);
    if (inBundle >= 0) {
      // Through Launch Services, so the launcher doesn't run inside this
      // app's sandbox.
      final bundle = launcher.substring(0, inBundle + '.app'.length);
      await Process.start('/usr/bin/open', [
        '-n',
        bundle,
        '--args',
        '--after-pid',
        '$pid',
      ], mode: ProcessStartMode.detached);
    } else {
      await Process.start(launcher, [
        '--after-pid',
        '$pid',
      ], mode: ProcessStartMode.detached);
    }
    exit(0);
  }
}

/// This app's install, overridden in `main`; null when it has none.
final installationProvider = Provider<Installation?>((ref) => null);

/// The track saved in `update.json`, read once at startup.
final releaseTrackProvider =
    NotifierProvider<ReleaseTrackNotifier, ReleaseTrack?>(
      ReleaseTrackNotifier.new,
    );

class ReleaseTrackNotifier extends Notifier<ReleaseTrack?> {
  @override
  ReleaseTrack? build() {
    final installation = ref.watch(installationProvider);
    if (installation == null) return null;
    unawaited(_load(installation));
    return null;
  }

  Future<void> _load(Installation installation) async {
    final track = await installation.readTrack();
    if (ref.mounted) state = track;
  }

  Future<void> set(ReleaseTrack track) async {
    final installation = ref.read(installationProvider);
    if (installation == null) return;
    await installation.writeTrack(track);
    state = track;
  }
}
