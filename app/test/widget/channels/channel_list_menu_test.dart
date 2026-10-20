import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inSidebar(String text) => find
    .descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text(text),
    )
    .first;

Future<void> _rightClick(WidgetTester tester, Offset at) async {
  await tester.tapAt(
    at,
    buttons: kSecondaryMouseButton,
    kind: PointerDeviceKind.mouse,
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

List<List<String>> _panels(WidgetTester tester) => [
  for (final panel in tester.widgetList<OcMenuPanel>(find.byType(OcMenuPanel)))
    [
      for (final entry in panel.entries)
        if (entry is OcMenuItem) entry.label,
    ],
];

void main() {
  testWidgets('a right-click between channels offers to create one, and a '
      'voice channel starts as voice', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1300));
    // Below the last channel row, in the empty part of the list.
    final below = tester.getBottomLeft(_inSidebar('Late night'));

    await _rightClick(tester, below + const Offset(20, 40));

    expect(_panels(tester), [
      [
        'Create text channel',
        'Create voice channel',
        'Create category',
        'Invite people',
      ],
    ]);
    await tester.tap(find.text('Create voice channel'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(find.byType(EditableText).last, 'Quiet room');
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Create channel'));
    await _pumpFor(tester, const Duration(seconds: 2));

    final created = app
        .read(serverProvider(_dev))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.name == 'Quiet room');
    expect(created.kind, ChannelKind.voice);
    expect(created.parentId, isNull);
    await app.dispose(tester);
  });

  testWidgets('on a category it creates the channel in it', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1300));

    await _rightClick(tester, tester.getCenter(_inSidebar('VOICE')));
    await tester.tap(find.text('Create text channel'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(find.byType(EditableText).last, 'notes');
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Create channel'));
    await _pumpFor(tester, const Duration(seconds: 2));

    final channels = app.read(serverProvider(_dev)).data!.channels.values;
    final voice = channels.firstWhere((channel) => channel.name == 'Voice');
    final created = channels.firstWhere((channel) => channel.name == 'notes');
    expect(created.kind, ChannelKind.text);
    expect(created.parentId, voice.id);
    await app.dispose(tester);
  });

  testWidgets('a channel keeps its own menu', (tester) async {
    final app = await MockApp.pump(tester);

    await _rightClick(tester, tester.getCenter(_inSidebar('help')));

    expect(_panels(tester), hasLength(1));
    expect(_panels(tester).single, isNot(contains('Create category')));
    await app.dispose(tester);
  });
}
