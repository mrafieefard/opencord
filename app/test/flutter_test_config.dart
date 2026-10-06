import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Runs before every test file: loads the bundled fonts so widget and
/// golden tests render real text and icons instead of the Ahem test font.
Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  TestWidgetsFlutterBinding.ensureInitialized();
  await _loadAppFonts();
  final comparator = goldenFileComparator;
  if (comparator is LocalFileComparator) {
    goldenFileComparator = _TolerantComparator(
      comparator.basedir.resolve('golden_test.dart'),
    );
  }
  await testMain();
}

const _fonts = {
  'Inter': [
    'Inter-Regular.ttf',
    'Inter-Italic.ttf',
    'Inter-Medium.ttf',
    'Inter-SemiBold.ttf',
    'Inter-SemiBoldItalic.ttf',
    'Inter-Bold.ttf',
  ],
  'JetBrainsMono': ['JetBrainsMono-Regular.ttf'],
  'MaterialSymbolsRounded': ['MaterialSymbolsRounded.ttf'],
};

Future<void> _loadAppFonts() async {
  for (final MapEntry(key: family, value: files) in _fonts.entries) {
    final loader = FontLoader(family);
    for (final file in files) {
      loader.addFont(
        File(
          'assets/fonts/$file',
        ).readAsBytes().then((bytes) => ByteData.sublistView(bytes)),
      );
    }
    await loader.load();
  }
}

/// Accepts goldens that differ in at most 0.5% of pixels, which absorbs
/// anti-aliasing changes between Flutter releases without hiding real
/// regressions.
class _TolerantComparator extends LocalFileComparator {
  _TolerantComparator(super.testFile);

  static const double _tolerance = 0.005;

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    if (result.passed || result.diffPercent <= _tolerance) {
      result.dispose();
      return true;
    }
    final error = await generateFailureOutput(result, golden, basedir);
    result.dispose();
    throw FlutterError(error);
  }
}
