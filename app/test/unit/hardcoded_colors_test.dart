import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// §13: colors come from the tokens in lib/ui/theme/ and nowhere else.
void main() {
  test('no hard-coded colors outside lib/ui/theme/', () {
    final forbidden = RegExp(
      r'Color\(0x|Color\.from(ARGB|RGBO)\(|\bColors\.(?!transparent\b)',
    );
    final offenders = <String>[];
    final sources = Directory('lib')
        .listSync(recursive: true)
        .whereType<File>()
        .where((file) => file.path.endsWith('.dart'))
        .where((file) => !file.path.startsWith('lib/ui/theme/'))
        .where((file) => !file.path.startsWith('lib/src/rust/'));
    for (final file in sources) {
      final lines = file.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        if (forbidden.hasMatch(lines[i])) {
          offenders.add('${file.path}:${i + 1}: ${lines[i].trim()}');
        }
      }
    }

    expect(offenders, isEmpty, reason: offenders.join('\n'));
  });
}
