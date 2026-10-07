import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

Future<void> _pump(WidgetTester tester, String text, double width) =>
    tester.pumpWidget(
      MaterialApp(
        home: Center(
          child: SizedBox(
            width: width,
            // Without a Material ancestor the fallback style is 48 px.
            child: EllipsisText(text, style: const TextStyle(fontSize: 14)),
          ),
        ),
      ),
    );

void main() {
  const long = 'a-very-long-channel-name-that-cannot-possibly-fit-here';

  testWidgets('a cut name shows in full in a tooltip (§16)', (tester) async {
    await _pump(tester, long, 120);

    final tooltip = tester.widget<Tooltip>(find.byType(Tooltip));
    expect(tooltip.message, long);
    final text = tester.widget<Text>(find.text(long));
    expect(text.overflow, TextOverflow.ellipsis);
    expect(text.maxLines, 1);
  });

  testWidgets('a name that fits has no tooltip', (tester) async {
    await _pump(tester, 'general', 300);

    expect(find.byType(Tooltip), findsNothing);
    expect(find.text('general'), findsOneWidget);
  });
}
