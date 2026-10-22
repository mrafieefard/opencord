import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/emoji_picker.dart';

/// Two sections, four columns: ten emoji (rows of 4, 4, 2), then five.
const lengths = [10, 5];

(int, int) move((int, int) at, LogicalKeyboardKey key) =>
    moveInGrid(lengths, at, key, columns: 4);

void main() {
  test('right and left run along a row and across sections', () {
    expect(move((0, 1), LogicalKeyboardKey.arrowRight), (0, 2));
    expect(move((0, 9), LogicalKeyboardKey.arrowRight), (1, 0));
    expect(move((1, 0), LogicalKeyboardKey.arrowLeft), (0, 9));
    expect(move((0, 0), LogicalKeyboardKey.arrowLeft), (0, 0));
  });

  test('down keeps the column, clamping to short rows', () {
    expect(move((0, 1), LogicalKeyboardKey.arrowDown), (0, 5));
    expect(move((0, 7), LogicalKeyboardKey.arrowDown), (0, 9));
    expect(move((0, 9), LogicalKeyboardKey.arrowDown), (1, 1));
    expect(move((1, 4), LogicalKeyboardKey.arrowDown), (1, 4));
  });

  test('up keeps the column, landing in the last row above', () {
    expect(move((0, 5), LogicalKeyboardKey.arrowUp), (0, 1));
    expect(move((1, 1), LogicalKeyboardKey.arrowUp), (0, 9));
    expect(move((1, 3), LogicalKeyboardKey.arrowUp), (0, 9));
    expect(move((0, 2), LogicalKeyboardKey.arrowUp), (0, 2));
  });

  test('empty sections are skipped', () {
    expect(
      moveInGrid([2, 0, 3], (0, 1), LogicalKeyboardKey.arrowRight, columns: 4),
      (2, 0),
    );
  });
}
