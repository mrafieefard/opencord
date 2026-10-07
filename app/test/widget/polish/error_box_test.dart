import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/widgets/error_box.dart';

class _Fails extends StatelessWidget {
  const _Fails();

  @override
  Widget build(BuildContext context) => throw StateError('broken');
}

void main() {
  testWidgets('a widget that fails to build takes one line in a list', (
    tester,
  ) async {
    final standard = ErrorWidget.builder;
    limitErrorBoxes();
    try {
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ListView(children: const [_Fails(), SizedBox(height: 20)]),
        ),
      );

      expect(tester.takeException(), isA<StateError>());
      expect(
        tester.getSize(find.byType(ErrorWidget)).height,
        lessThanOrEqualTo(48),
      );
    } finally {
      // The test binding checks it is back before tear-downs run.
      ErrorWidget.builder = standard;
    }
  });
}
