import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/attachments/file_card.dart';
import 'package:opencord/features/chat/attachments/image_grid.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

Message _msg(List<Attachment> attachments) => Message(
  id: 1,
  channelId: 10,
  authorId: 100,
  content: '',
  createdAt: DateTime(2026, 1, 1),
  attachments: attachments,
);

void main() {
  testWidgets('one_image_reserves_its_box_before_loading', (tester) async {
    final completer = Completer<Uint8List>();
    final attachment = const Attachment(
      id: 10,
      filename: 'wide.png',
      contentType: 'image/png',
      sizeBytes: 50000,
      kind: AttachmentKind.image,
      width: 1600,
      height: 900,
      hasThumbnail: true,
      thumbnailWidth: 800,
      thumbnailHeight: 450,
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          attachmentBytesProvider.overrideWith((ref, arg) => completer.future),
        ],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg([attachment]),
            ),
          ),
        ),
      ),
    );

    // Box size is 400x225 BEFORE the future completes
    final gridFinder = find.byType(ImageGrid);
    expect(gridFinder, findsOneWidget);
    final size = tester.getSize(gridFinder);
    expect(size.width, 400.0);
    expect(size.height, 225.0);

    // Completer finishes
    completer.complete(Uint8List(0));
    await tester.pump();
  });

  testWidgets('four_plus_images_show_the_counter', (tester) async {
    final attachments = [
      for (var i = 1; i <= 6; i++)
        Attachment(
          id: i,
          filename: 'photo_$i.png',
          contentType: 'image/png',
          sizeBytes: 10000,
          kind: AttachmentKind.image,
          width: 800,
          height: 600,
          hasThumbnail: true,
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
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg(attachments),
            ),
          ),
        ),
      ),
    );

    await tester.pump();

    // 6 images -> 4th cell shows +2
    expect(find.text('+2'), findsOneWidget);
  });

  testWidgets('file_card_shows_preview_only_for_small_text', (tester) async {
    final smallText = const Attachment(
      id: 1,
      filename: 'readme.txt',
      contentType: 'text/plain',
      sizeBytes: 100 * 1024, // 100 KiB <= 256 KiB
      kind: AttachmentKind.file,
    );
    final largeText = const Attachment(
      id: 2,
      filename: 'huge.txt',
      contentType: 'text/plain',
      sizeBytes: 300 * 1024, // 300 KiB > 256 KiB
      kind: AttachmentKind.file,
    );
    final binaryFile = const Attachment(
      id: 3,
      filename: 'archive.zip',
      contentType: 'application/zip',
      sizeBytes: 50 * 1024,
      kind: AttachmentKind.file,
    );

    await tester.pumpWidget(
      ProviderScope(
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: Column(
              children: [
                FileCard(serverKey: 'server', attachment: smallText),
                FileCard(serverKey: 'server', attachment: largeText),
                FileCard(serverKey: 'server', attachment: binaryFile),
              ],
            ),
          ),
        ),
      ),
    );

    // Only smallText gets a "Preview" button
    expect(find.text('Preview'), findsOneWidget);
    // All 3 get a Download button
    expect(find.text('Download'), findsNWidgets(3));
  });
}
