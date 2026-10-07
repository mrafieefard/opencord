import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/features/voice/voice_layout.dart';

void main() {
  test('each participant gets a tile, and a screenshare one more', () {
    final tiles = voiceTiles(const [
      VoiceParticipant(userId: 1),
      VoiceParticipant(userId: 2, screensharing: true),
      VoiceParticipant(userId: 3, camera: true),
    ]);

    expect(tiles, [
      (userId: 1, screen: false),
      (userId: 2, screen: false),
      (userId: 2, screen: true),
      (userId: 3, screen: false),
    ]);
  });

  test('columns grow with the number of tiles (§4.10)', () {
    expect(
      [
        for (final count in [1, 2, 4, 5, 9, 10, 25]) gridColumns(count),
      ],
      [1, 2, 2, 3, 3, 4, 4],
    );
  });

  group('tiles fit without scrolling', () {
    test('a wide area is limited by its height', () {
      final tile = fitTiles(const Size(2000, 900), 1, gap: 0);

      expect(tile, const Size(1600, 900));
    });

    test('a tall area is limited by its width', () {
      final tile = fitTiles(const Size(1000, 1000), 4, gap: 0);

      expect(tile.width, 500);
      expect(tile.height, closeTo(281.25, 0.001));
    });

    test('gaps are left between tiles', () {
      final tile = fitTiles(const Size(1010, 2000), 4, gap: 10);

      expect(tile.width, 500);
    });

    test('every row fits in the height', () {
      const area = Size(1600, 600);
      for (final count in [1, 3, 7, 12, 30]) {
        final tile = fitTiles(area, count, gap: 8);
        final rows = (count / gridColumns(count)).ceil();
        expect(tile.height * rows + 8 * (rows - 1), lessThanOrEqualTo(600));
        expect(tile.width / tile.height, closeTo(16 / 9, 0.001));
      }
    });

    test('no tiles or no room give an empty tile', () {
      expect(fitTiles(const Size(800, 600), 0), Size.zero);
      expect(fitTiles(const Size(4, 4), 9, gap: 8), Size.zero);
    });
  });
}
