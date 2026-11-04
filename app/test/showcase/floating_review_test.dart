// The prototype's scenes rendered from the app, for the floating UI's
// checkpoints. Skipped unless OPENCORD_REVIEW names a folder:
//
//   flutter test --dart-define=OPENCORD_REVIEW=<folder> \
//     --dart-define=OPENCORD_REVIEW_SCENES=text,voice \
//     --dart-define=OPENCORD_REVIEW_WIDTHS=wide-1280,medium-980 \
//     --dart-define=OPENCORD_EMOJI=<a colour emoji font> \
//     test/showcase/floating_review_test.dart
//
// Only OPENCORD_REVIEW is needed. Writes <scene>_<width>_<theme>.png and
// merges each scene's frames into the folder's measure.json, keyed as
// `reference/measure.json` is, for `tools/compare.py`.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';

import '../support/app.dart';
import '../support/floating_scenes.dart';

const _out = String.fromEnvironment('OPENCORD_REVIEW');

/// Adds one scene's frames at one width to measure.json, keeping what the
/// folder already holds.
void _record(String scene, Map<String, List<double>> frames) {
  final file = File('$_out/measure.json');
  final all = file.existsSync()
      ? jsonDecode(file.readAsStringSync()) as Map<String, Object?>
      : <String, Object?>{};
  all[scene] = frames;
  file.writeAsStringSync(jsonEncode(all));
}

void main() {
  final scenes = defineList(
    const String.fromEnvironment('OPENCORD_REVIEW_SCENES'),
  );
  final widths = defineList(
    const String.fromEnvironment('OPENCORD_REVIEW_WIDTHS'),
  );

  setUpAll(() async {
    if (_out.isEmpty) return;
    Directory(_out).createSync(recursive: true);
    await loadEmojiFont(const String.fromEnvironment('OPENCORD_EMOJI'));
  });

  for (final scene in floatingScenes) {
    if (scenes != null && !scenes.contains(scene.name)) continue;
    for (final width in scene.widths) {
      if (widths != null && !widths.contains(width.tag)) continue;
      for (final theme in [
        ThemePreference.dark,
        if (scene.light.contains(width)) ThemePreference.light,
      ]) {
        final shot = '${scene.name}_${width.tag}_${theme.name}';
        testWidgets(shot, skip: _out.isEmpty, (tester) async {
          final app = await MockApp.pump(
            tester,
            size: width.size,
            platform: TargetPlatform.linux,
            settings: (s) => s.copyWith(theme: theme),
          );
          final driver = SceneDriver(tester);
          try {
            await driver.settle(600);
            await scene.play(driver);
          } on SceneUnavailable catch (missing) {
            debugPrint('skipped $shot: $missing');
            await app.dispose(tester);
            return;
          }
          await driver.settle(600);
          await decodeImages(tester);
          await writeShot(tester, '$_out/$shot.png');
          if (theme == ThemePreference.dark) {
            _record('${scene.name}@${width.tag}', measureFrames(tester));
          }
          await app.dispose(tester);
        });
      }
    }
  }
}
