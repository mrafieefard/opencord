// The prototype's animations, frozen at the times `film/` holds, from the
// app at 1280 × 800, for the floating UI's checkpoints. Skipped unless
// OPENCORD_FILM names a folder:
//
//   flutter test --dart-define=OPENCORD_FILM=<folder> \
//     --dart-define=OPENCORD_REVIEW_SCENES=pane-open,pane-close \
//     test/showcase/floating_film_test.dart
//
// Writes <scene>_<ms>.png, cut to the part of the window the prototype's
// frame shows (`tools/anim.mjs`), so the two compare side by side.
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';

import '../support/app.dart';
import '../support/floating_scenes.dart';

const _out = String.fromEnvironment('OPENCORD_FILM');

/// An animation: where it starts, the one action that starts it, the part
/// of the window it shows and the times of its frames.
class _Film {
  const _Film(this.name, this.setup, this.action, this.clip, this.times);

  final String name;
  final Future<void> Function(SceneDriver scene) setup;
  final Future<void> Function(SceneDriver scene) action;
  final Rect clip;
  final List<int> times;
}

const _times = [0, 40, 80, 120, 160, 240, 360];
const _tileTimes = [0, 40, 80, 120, 200, 320];
const _paneClip = Rect.fromLTWH(900, 0, 380, 220);
const _mainClip = Rect.fromLTWH(374, 40, 634, 760);

Future<void> _closePane(SceneDriver scene) =>
    scene.tap('the pane\'s close button', scene.paneClose);

Future<void> _openPane(SceneDriver scene) => scene.tapPaneButton();

Future<void> _leaveVoice(SceneDriver scene) =>
    scene.tap('the voice card\'s leave button', scene.voiceCardLeave);

/// The text scene, with General scrolled into view to be tapped.
Future<void> _textGeneralShown(SceneDriver scene) async {
  await playText(scene);
  await scene.revealInLeft('General');
}

Future<void> _openVoice(SceneDriver scene) =>
    scene.tap('General', scene.inLeft('General'));

Future<void> _watch(SceneDriver scene) =>
    scene.tap('Watch', [find.text('Watch')]);

Future<void> _unfocus(SceneDriver scene) =>
    scene.tap('the tile shown large', scene.focusedTile);

Future<void> _music(SceneDriver scene) =>
    scene.tap('Music in the right pane', scene.paneTab('Music'));

Future<void> _serverMenu(SceneDriver scene) =>
    scene.tap('the server\'s head', scene.serverHead);

const _films = [
  _Film('pane-open', playTextPaneClosed, _openPane, _paneClip, _times),
  _Film('pane-close', playText, _closePane, _paneClip, _times),
  _Film(
    'voice-leave',
    playText,
    _leaveVoice,
    Rect.fromLTWH(70, 520, 310, 280),
    _times,
  ),
  _Film('voice-open', _textGeneralShown, _openVoice, _mainClip, _tileTimes),
  _Film('tile-focus', openVoicePage, _watch, _mainClip, _tileTimes),
  _Film('tile-unfocus', watchStream, _unfocus, _mainClip, _tileTimes),
  _Film(
    'music-swap',
    openVoicePage,
    _music,
    Rect.fromLTWH(900, 0, 380, 400),
    _times,
  ),
  _Film(
    'server-menu',
    playText,
    _serverMenu,
    Rect.fromLTWH(70, 30, 320, 330),
    _times,
  ),
];

void main() {
  final names = defineList(
    const String.fromEnvironment('OPENCORD_REVIEW_SCENES'),
  );

  setUpAll(() async {
    if (_out.isEmpty) return;
    Directory(_out).createSync(recursive: true);
    await loadEmojiFont(const String.fromEnvironment('OPENCORD_EMOJI'));
  });

  for (final film in _films) {
    if (names != null && !names.contains(film.name)) continue;
    testWidgets(film.name, skip: _out.isEmpty, (tester) async {
      final app = await MockApp.pump(
        tester,
        size: wide1280.size,
        platform: TargetPlatform.linux,
        settings: (s) => s.copyWith(theme: ThemePreference.dark),
      );
      final scene = SceneDriver(tester);
      try {
        await scene.settle(600);
        await film.setup(scene);
        await scene.settle(600);
        await decodeImages(tester);
        await film.action(scene);
      } on SceneUnavailable catch (missing) {
        debugPrint('skipped ${film.name}: $missing');
        await app.dispose(tester);
        return;
      }
      // The first frame builds what the action changed and starts its
      // animations; the second is their first tick, at 0 ms.
      await tester.pump();
      await tester.pump();
      var at = 0;
      for (final time in film.times) {
        await tester.pump(Duration(milliseconds: time - at));
        at = time;
        final ms = '$time'.padLeft(3, '0');
        await writeShot(tester, '$_out/${film.name}_$ms.png', clip: film.clip);
      }
      await scene.settle(600);
      await app.dispose(tester);
    });
  }
}
