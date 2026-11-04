import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/widgets/notched_float.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

import '../support/pump.dart';

const _float = Size(220, 170);

/// A main page's header: 52 including its bottom line, as the notch's
/// bottom lies on that line. It would paint into the notch if the clip
/// didn't hold.
class _Header extends StatelessWidget {
  const _Header();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Align(
      alignment: Alignment.topCenter,
      child: Container(
        height: 52,
        decoration: BoxDecoration(
          color: colors.sidebar,
          border: Border(bottom: BorderSide(color: colors.border)),
        ),
      ),
    );
  }
}

void main() {
  for (final (name, colors) in themes) {
    testWidgets('floats and the notch at 52, 26 and 0 ($name)', (tester) async {
      // At 2×, so the curves can be checked at full size.
      const logical = Size(16 + 4 * 220 + 3 * 12 + 16, 16 + 170 + 28);
      tester.view.physicalSize = logical * 2;
      tester.view.devicePixelRatio = 2;
      addTearDown(tester.view.reset);
      final notches = [
        for (final n in [52.0, 26.0, 0.0]) ValueNotifier(n),
      ];
      addTearDown(() {
        for (final notch in notches) {
          notch.dispose();
        }
      });

      await pumpThemed(
        tester,
        colors: colors,
        background: (c) => c.rail,
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 28),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            spacing: 12,
            children: [
              SizedBox.fromSize(
                size: _float,
                child: const OcFloat(child: SizedBox.expand()),
              ),
              for (final notch in notches)
                SizedBox.fromSize(
                  size: _float,
                  child: NotchedFloat(notch: notch, child: const _Header()),
                ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/floats_$name.png'),
      );
    });
  }
}
