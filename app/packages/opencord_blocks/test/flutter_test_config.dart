import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Runs before every test file: loads the app's Inter (from `app/assets`) so
/// goldens show real text, and accepts goldens within 0.5% of their pixels,
/// as the app's tests do.
Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  TestWidgetsFlutterBinding.ensureInitialized();
  final inter = FontLoader('Inter');
  for (final file in const [
    'Inter-Regular.ttf',
    'Inter-Italic.ttf',
    'Inter-Medium.ttf',
    'Inter-SemiBold.ttf',
    'Inter-Bold.ttf',
  ]) {
    inter.addFont(
      File(
        '../../assets/fonts/$file',
      ).readAsBytes().then((bytes) => ByteData.sublistView(bytes)),
    );
  }
  await inter.load();
  final symbols = FontLoader('MaterialSymbolsRounded');
  symbols.addFont(
    File(
      '../../assets/fonts/MaterialSymbolsRounded.ttf',
    ).readAsBytes().then((bytes) => ByteData.sublistView(bytes)),
  );
  await symbols.load();
  final comparator = goldenFileComparator;
  if (comparator is LocalFileComparator) {
    goldenFileComparator = _TolerantComparator(
      comparator.basedir.resolve('golden_test.dart'),
    );
  }
  await testMain();
}

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
