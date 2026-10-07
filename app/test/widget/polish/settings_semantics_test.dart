import 'dart:ui' show Tristate;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../../support/pump.dart';

void main() {
  testWidgets('a switch row says what it is, and whether it is on (§9)', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await pumpThemed(
      tester,
      Material(
        child: SettingsSwitchRow(
          title: 'Display separately',
          subtitle: 'Members with this role get their own section',
          value: true,
          onChanged: (_) {},
        ),
      ),
    );

    final node = tester.getSemantics(
      find.bySemanticsLabel(RegExp('Display separately')),
    );
    final data = node.getSemanticsData();
    expect(data.label, contains('their own section'));
    expect(data.flagsCollection.isToggled, Tristate.isTrue);
    semantics.dispose();
  });

  testWidgets('buttons in a row are there for screen readers', (tester) async {
    final semantics = tester.ensureSemantics();
    await pumpThemed(
      tester,
      Material(
        child: SettingsRow(
          title: 'Public key',
          subtitle: 'ab12 cd34',
          trailing: OcButton(label: 'Copy public key', onPressed: () {}),
        ),
      ),
    );

    expect(find.bySemanticsLabel('Copy public key'), findsOneWidget);
    expect(find.bySemanticsLabel(RegExp('ab12 cd34')), findsOneWidget);
    semantics.dispose();
  });
}
