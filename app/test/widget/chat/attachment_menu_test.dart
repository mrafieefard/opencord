import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/attachments/file_card.dart';
import 'package:opencord/features/chat/attachments/image_grid.dart';
import 'package:opencord/features/chat/voice/voice_bubble.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<int> _open(WidgetTester tester, MockApp app, String name) async {
  final id = app
      .read(serverProvider(_server))
      .data!
      .channels
      .values
      .firstWhere((channel) => channel.name == name)
      .id;
  app.read(navigationProvider.notifier).openChannel(_server, id);
  await _pumpFor(tester, const Duration(milliseconds: 300));
  return id;
}

Future<void> _rightClick(WidgetTester tester, Finder target) async {
  await tester.tapAt(
    tester.getCenter(target),
    buttons: kSecondaryMouseButton,
    kind: PointerDeviceKind.mouse,
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

List<List<String>> _panels(WidgetTester tester) => [
  for (final panel in tester.widgetList<OcMenuPanel>(find.byType(OcMenuPanel)))
    [
      for (final entry in panel.entries)
        if (entry is OcMenuItem) entry.label,
    ],
];

void main() {
  testWidgets('a right-click on a picture opens one menu: its and the '
      'message’s', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'design-review');

    await _rightClick(tester, find.byType(ImageGrid));

    final panels = _panels(tester);
    expect(panels, hasLength(1));
    expect(panels.single.take(3), ['Copy image', 'Save image as…', 'Reply']);
    expect(panels.single, contains('Copy text'));
    await app.dispose(tester);
  });

  testWidgets('a voice message offers to save it, and no text to copy', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'design-review');

    await _rightClick(tester, find.byType(VoiceBubble));

    final panels = _panels(tester);
    expect(panels, hasLength(1));
    expect(panels.single.first, 'Save voice message as…');
    expect(panels.single, isNot(contains('Copy text')));
    await app.dispose(tester);
  });

  testWidgets('a message with several files has a Download submenu', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    final sent = app.repository.sendMessageWithFiles(
      _server,
      channel,
      'Both drafts',
      nonce: 'two-files',
      files: const [
        StagedFile(id: 'a', name: 'draft-1.pdf', size: 1000),
        StagedFile(id: 'b', name: 'draft-2.pdf', size: 2000),
      ],
    );
    await _pumpFor(tester, const Duration(seconds: 2));
    await sent;

    // On one file: that file first, then all of them.
    await _rightClick(tester, find.byType(FileCard).first);
    expect(_panels(tester).single.take(2), ['Save file as…', 'Download']);

    await tester.tap(
      find.descendant(
        of: find.byType(OcMenuPanel),
        matching: find.text('Download'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(_panels(tester).last, [
      'All 2 files…',
      'draft-1.pdf',
      'draft-2.pdf',
    ]);
    await app.dispose(tester);
  });
}
