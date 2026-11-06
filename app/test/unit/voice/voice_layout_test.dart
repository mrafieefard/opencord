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
      final tile = fitTiles(const Size(1000, 300), 4, gap: 0);

      expect(tile, const Size(240, 150));
    });

    test('a tall area is limited by its width', () {
      final tile = fitTiles(const Size(1000, 1000), 4, gap: 0);

      expect(tile.width, 500);
      expect(tile.height, closeTo(312.5, 0.001));
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
        expect(tile.width / tile.height, closeTo(16 / 10, 0.001));
      }
    });

    test('no tiles or no room give an empty tile', () {
      expect(fitTiles(const Size(800, 600), 0), Size.zero);
      expect(fitTiles(const Size(4, 4), 9, gap: 8), Size.zero);
    });

    test('a group is at most 1040 wide, or 520 under 3 tiles', () {
      const area = Size(2000, 2000);

      expect(fitTiles(area, 1).width, 520);
      expect(fitTiles(area, 2).width, 256);
      expect(fitTiles(area, 3).width, 516);
      expect(fitTiles(area, 6).width, closeTo(1024 / 3, 0.001));
    });
  });

  group('columns fit the width', () {
    test('as many as gridColumns says while each is at least 190', () {
      expect(tileColumns(6, 600), 3);
      expect(tileColumns(6, 586), 3);
      expect(tileColumns(6, 585), 2);
      expect(tileColumns(4, 1040), 2);
      expect(tileColumns(6, 100), 1);
    });
  });

  group('gridLayout', () {
    test('6 tiles at 1280 are the prototype\'s', () {
      final layout = gridLayout(const Size(600, 674), 6);

      expect(layout.group.left, 0);
      expect(layout.group.width, closeTo(600, 0.001));
      expect(layout.group.height, closeTo(251.33, 0.01));
      expect(layout.group.top, closeTo(211.33, 0.01));
      expect(layout.tiles.last.left, closeTo(405.33, 0.01));
      expect(layout.tiles.last.top, closeTo(129.67, 0.01));
    });

    test('a short last row starts at the left', () {
      final layout = gridLayout(const Size(600, 674), 5);

      expect(layout.tiles[3].left, 0);
      expect(layout.tiles[4].left, closeTo(202.67, 0.01));
    });

    test('a group narrower than its stage is centred', () {
      final layout = gridLayout(const Size(1400, 800), 2);

      expect(layout.group.width, 520);
      expect(layout.group.left, 440);
    });

    test('no tiles have no group', () {
      final layout = gridLayout(const Size(600, 600), 0);

      expect(layout.tiles, isEmpty);
      expect(layout.group.size, Size.zero);
    });
  });

  group('focusLayout', () {
    const area = Size(608, 682);

    test('the large one fills above a 76 strip, 8 between', () {
      final layout = focusLayout(area, 5);

      expect(layout.large, const Rect.fromLTWH(0, 0, 608, 598));
      expect(layout.strip, const Rect.fromLTWH(0, 606, 608, 76));
      expect(layout.stripTile.width, closeTo(115.2, 0.001));
      expect(layout.stripTile.height, 76);
    });

    test('strip tiles are at most 200 and at least 96', () {
      expect(focusLayout(area, 1).stripTile.width, 200);
      expect(focusLayout(area, 20).stripTile.width, 96);
    });

    test('without others there is no strip', () {
      final layout = focusLayout(area, 0);

      expect(layout.large, Offset.zero & area);
      expect(layout.strip, Rect.zero);
    });

    test('a stage shorter than a strip stays laid out', () {
      final layout = focusLayout(const Size(608, 40), 3);

      expect(layout.large.height, 0);
      expect(layout.strip.height, 40);
    });
  });
}
