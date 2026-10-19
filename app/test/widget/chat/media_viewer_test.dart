import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

void main() {
  testWidgets('viewer_navigates_and_closes', (tester) async {
    final media = [
      const Attachment(
        id: 1,
        filename: 'img1.png',
        contentType: 'image/png',
        sizeBytes: 1000,
        kind: AttachmentKind.image,
      ),
      const Attachment(
        id: 2,
        filename: 'img2.png',
        contentType: 'image/png',
        sizeBytes: 2000,
        kind: AttachmentKind.image,
      ),
      const Attachment(
        id: 3,
        filename: 'img3.png',
        contentType: 'image/png',
        sizeBytes: 3000,
        kind: AttachmentKind.image,
      ),
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          attachmentBytesProvider.overrideWith(
            (ref, arg) async => Uint8List(0),
          ),
        ],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: Builder(
              builder: (context) => ElevatedButton(
                onPressed: () => showMediaViewer(
                  context,
                  serverKey: 'server.example:7710',
                  media: media,
                  index: 0,
                ),
                child: const Text('Open Viewer'),
              ),
            ),
          ),
        ),
      ),
    );

    // Open the viewer
    await tester.tap(find.text('Open Viewer'));
    await tester.pumpAndSettle();

    // Index text initially shows '1 / 3'
    expect(find.text('1 / 3'), findsOneWidget);
    expect(find.text('img1.png'), findsOneWidget);

    // Arrow right moves to second item
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(find.text('2 / 3'), findsOneWidget);
    expect(find.text('img2.png'), findsOneWidget);

    // Arrow right moves to third item
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(find.text('3 / 3'), findsOneWidget);
    expect(find.text('img3.png'), findsOneWidget);

    // Arrow left moves back to second item
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pump();
    expect(find.text('2 / 3'), findsOneWidget);
    expect(find.text('img2.png'), findsOneWidget);

    // Esc closes the viewer
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();

    expect(find.text('2 / 3'), findsNothing);
    expect(find.text('Open Viewer'), findsOneWidget);
  });
}
