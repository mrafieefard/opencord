import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/voice/voice_tile.dart';

import '../support/pump.dart';

const _tile = Size(240, 135);

Widget _sized(Widget tile, {Size size = _tile}) =>
    SizedBox.fromSize(size: size, child: tile);

void main() {
  for (final (name, colors) in themes) {
    testWidgets('voice tile states ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(4 * 240 + 5 * 8, 2 * 135 + 3 * 8),
        Padding(
          padding: const EdgeInsets.all(8),
          child: Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _sized(const VoiceTileView(userId: 1, name: 'Mira Okafor')),
              _sized(
                const VoiceTileView(
                  userId: 2,
                  name: 'Kai Nakamura',
                  speaking: true,
                ),
              ),
              _sized(
                const VoiceTileView(
                  userId: 3,
                  name: 'Jonas Weber',
                  muted: true,
                ),
              ),
              _sized(
                const VoiceTileView(
                  userId: 4,
                  name: 'Lena Fischer',
                  muted: true,
                  deafened: true,
                ),
              ),
              _sized(
                const VoiceTileView(
                  userId: 5,
                  name: 'Priya Shah',
                  camera: true,
                  speaking: true,
                ),
              ),
              _sized(
                const VoiceTileView(
                  userId: 6,
                  name: 'Tomás Ruiz',
                  screen: true,
                ),
              ),
              _sized(
                const VoiceTileView(
                  userId: 7,
                  name: 'Ava Lindqvist',
                  small: true,
                  muted: true,
                ),
                size: const Size(160, 90),
              ),
              _sized(
                const VoiceTileView(
                  userId: 6,
                  name: 'Tomás Ruiz',
                  screen: true,
                  small: true,
                ),
                size: const Size(160, 90),
              ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/voice_tiles_$name.png'),
      );
    });
  }
}
