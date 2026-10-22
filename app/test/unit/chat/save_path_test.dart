import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/attachments/download.dart';

void main() {
  final sep = Platform.pathSeparator;

  test('a name from the server cannot leave the folder', () {
    expect(safeFileName('../../.bashrc'), '.bashrc');
    expect(safeFileName(r'..\..\evil.exe'), 'evil.exe');
    expect(safeFileName('..'), 'file');
    expect(safeFileName('a\u0000b\nc.txt'), 'abc.txt');
    expect(safeFileName('  '), 'file');
  });

  test('a taken name gets a number before its extension', () {
    final taken = {'${sep}d${sep}photo.png', '${sep}d${sep}photo (2).png'};
    expect(
      freePathIn('${sep}d', 'photo.png', exists: taken.contains),
      '${sep}d${sep}photo (3).png',
    );
    expect(
      freePathIn('${sep}d$sep', 'notes', exists: (_) => false),
      '${sep}d${sep}notes',
    );
    expect(
      freePathIn('${sep}d', '.env', exists: {'${sep}d$sep.env'}.contains),
      '${sep}d$sep.env (2)',
    );
  });
}
