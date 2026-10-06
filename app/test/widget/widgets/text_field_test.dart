import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';

import '../../support/pump.dart';

void main() {
  testWidgets('shows its hint and submits on Enter', (tester) async {
    String? submitted;
    await pumpThemed(
      tester,
      Material(
        type: MaterialType.transparency,
        child: Center(
          child: SizedBox(
            width: 300,
            child: OcTextField(
              hint: 'Invite link or host:port',
              autofocus: true,
              onSubmitted: (value) => submitted = value,
            ),
          ),
        ),
      ),
    );

    expect(find.text('Invite link or host:port'), findsOneWidget);
    await tester.enterText(find.byType(EditableText), 'chat.example.org:7710');
    await tester.testTextInput.receiveAction(TextInputAction.done);

    expect(submitted, 'chat.example.org:7710');
  });

  testWidgets('mono fields use JetBrains Mono', (tester) async {
    await pumpThemed(
      tester,
      Material(
        type: MaterialType.transparency,
        child: Center(
          child: SizedBox(
            width: 300,
            child: OcTextField(
              controller: TextEditingController(text: 'ABCD-EFGH'),
              mono: true,
              readOnly: true,
            ),
          ),
        ),
      ),
    );

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.style.fontFamily, OcText.monoFamily);
  });

  testWidgets('errors are text with an outlined icon, never a color', (
    tester,
  ) async {
    await pumpThemed(
      tester,
      const InlineError('Could not connect: connection refused'),
    );

    final icon = tester.widget<Icon>(find.byIcon(OcIcons.error));
    final text = tester.widget<Text>(
      find.text('Could not connect: connection refused'),
    );
    expect(icon.fill, 0);
    expect(icon.color, OcColors.dark.text);
    expect(text.style?.color, OcColors.dark.text);
  });
}
