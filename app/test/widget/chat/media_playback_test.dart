import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/attachments/audio_card.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/chat/attachments/video_player_view.dart';
import 'package:opencord/features/chat/attachments/video_tile.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

Message _msg(List<Attachment> attachments) => Message(
  id: 1,
  channelId: 10,
  authorId: 100,
  content: '',
  createdAt: DateTime(2026, 1, 1),
  attachments: attachments,
);

void main() {
  testWidgets('video_tile_opens_the_viewer_and_plays', (tester) async {
    final mockRepo = MockRepository(random: Random(1), simulateLife: false);
    final videoAttachment = const Attachment(
      id: 101,
      filename: 'clip.mp4',
      contentType: 'video/mp4',
      sizeBytes: 1000000,
      kind: AttachmentKind.video,
      width: 640,
      height: 360,
      durationMs: 15000,
      hasThumbnail: false,
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(mockRepo)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg([videoAttachment]),
            ),
          ),
        ),
      ),
    );

    // VideoTile is displayed with duration badge '0:15'
    expect(find.byType(VideoTile), findsOneWidget);
    expect(find.text('0:15'), findsOneWidget);

    // Tap video tile to open viewer
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();

    // MediaViewer and VideoPlayerView are opened
    expect(find.byType(MediaViewer), findsOneWidget);
    expect(find.byType(VideoPlayerView), findsOneWidget);
    expect(find.byType(Texture), findsOneWidget);

    // Playback started
    final playback = ProviderScope.containerOf(
      tester.element(find.byType(VideoPlayerView)),
    ).read(playbackProvider);
    expect(playback.playing, isTrue);
  });

  testWidgets('audio_card_plays_and_seeks', (tester) async {
    final mockRepo = MockRepository(random: Random(1), simulateLife: false);
    final audioAttachment = const Attachment(
      id: 202,
      filename: 'track.mp3',
      contentType: 'audio/mpeg',
      sizeBytes: 500000,
      kind: AttachmentKind.audio,
      durationMs: 60000,
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(mockRepo)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg([audioAttachment]),
            ),
          ),
        ),
      ),
    );

    // AudioCard is displayed
    expect(find.byType(AudioCard), findsOneWidget);
    expect(find.text('track.mp3'), findsOneWidget);

    // Tap play button
    final playBtn = find.descendant(
      of: find.byType(AudioCard),
      matching: find.byIcon(OcIcons.playArrow),
    );
    expect(playBtn, findsOneWidget);
    await tester.tap(playBtn);
    await tester.pumpAndSettle();

    // Now playing, pause icon appears
    expect(
      find.descendant(
        of: find.byType(AudioCard),
        matching: find.byIcon(OcIcons.pause),
      ),
      findsOneWidget,
    );

    // Seek: the time moves to where the slider was let go.
    expect(find.text('0:00 / 1:00'), findsOneWidget);
    final slider = find.byType(Slider);
    expect(slider, findsOneWidget);
    await tester.drag(slider, const Offset(50, 0));
    await tester.pump();
    expect(find.text('0:00 / 1:00'), findsNothing);
    final playback = ProviderScope.containerOf(
      tester.element(find.byType(AudioCard)),
    ).read(playbackProvider);
    expect(playback.positionMs, greaterThan(0));
    mockRepo.mediaClose();
  });

  testWidgets('the_viewer_steps_from_a_video_to_the_messages_image', (
    tester,
  ) async {
    final mockRepo = MockRepository(random: Random(1), simulateLife: false);
    const image = Attachment(
      id: 401,
      filename: 'photo.png',
      contentType: 'image/png',
      sizeBytes: 1000,
      kind: AttachmentKind.image,
      width: 64,
      height: 64,
    );
    const video = Attachment(
      id: 402,
      filename: 'clip.mp4',
      contentType: 'video/mp4',
      sizeBytes: 1000000,
      kind: AttachmentKind.video,
      width: 640,
      height: 360,
      durationMs: 15000,
    );
    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(mockRepo)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg([image, video]),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();
    expect(find.byType(VideoPlayerView), findsOneWidget);
    final container = ProviderScope.containerOf(
      tester.element(find.byType(MediaViewer)),
    );
    expect(container.read(playbackProvider).playing, isTrue);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pumpAndSettle();
    expect(find.byType(VideoPlayerView), findsNothing);
    expect(container.read(playbackProvider).item, isNull);
  });

  testWidgets('a_video_a_voice_message_interrupted_plays_again', (
    tester,
  ) async {
    final mockRepo = MockRepository(random: Random(1), simulateLife: false);
    const video = Attachment(
      id: 501,
      filename: 'clip.mp4',
      contentType: 'video/mp4',
      sizeBytes: 1000000,
      kind: AttachmentKind.video,
      width: 640,
      height: 360,
      durationMs: 15000,
    );
    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(mockRepo)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg([video]),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();
    final container = ProviderScope.containerOf(
      tester.element(find.byType(VideoPlayerView)),
    );
    const item = AttachmentPlayback('server.example:7710', 501);
    expect(container.read(playbackProvider).item, item);

    // A voice message starts: the video pauses and shows play.
    await container
        .read(playbackProvider.notifier)
        .play(const AttachmentPlayback('server.example:7710', 777));
    await tester.pump();
    expect(
      find.descendant(
        of: find.byType(VideoPlayerView),
        matching: find.byIcon(OcIcons.playArrow),
      ),
      findsOneWidget,
    );

    await tester.tap(
      find.descendant(
        of: find.byType(VideoPlayerView),
        matching: find.byIcon(OcIcons.playArrow),
      ),
    );
    await tester.pump();
    final playback = container.read(playbackProvider);
    expect(playback.item, item);
    expect(playback.playing, isTrue);
    container.read(playbackProvider.notifier).mediaClose();
  });

  testWidgets('unsupported_video_offers_download', (tester) async {
    final mockRepo = MockRepository(random: Random(1), simulateLife: false);
    mockRepo.mockMediaSupported = false;

    final videoAttachment = const Attachment(
      id: 303,
      filename: 'unsupported.mp4',
      contentType: 'video/mp4',
      sizeBytes: 2000000,
      kind: AttachmentKind.video,
      width: 640,
      height: 360,
      durationMs: 10000,
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(mockRepo)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg([videoAttachment]),
            ),
          ),
        ),
      ),
    );

    // Tap video tile to open viewer
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();

    // Unsupported message and Download button are visible
    expect(
      find.text("This video can't play on this computer yet"),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(VideoPlayerView),
        matching: find.widgetWithText(OcButton, 'Download'),
      ),
      findsOneWidget,
    );
  });

  testWidgets('a_video_tile_fills_its_box_whatever_size_was_sent', (
    tester,
  ) async {
    Future<Size> tileFor(int width, int height) async {
      await tester.pumpWidget(
        ProviderScope(
          overrides: [repositoryProvider.overrideWithValue(MockRepository())],
          child: MaterialApp(
            theme: buildTheme(OcColors.dark),
            home: Scaffold(
              body: AttachmentsView(
                serverKey: 'server.example:7710',
                message: _msg([
                  Attachment(
                    id: 404,
                    filename: 'clip.mp4',
                    contentType: 'video/mp4',
                    sizeBytes: 1000,
                    kind: AttachmentKind.video,
                    width: width,
                    height: height,
                  ),
                ]),
              ),
            ),
          ),
        ),
      );
      return tester.getSize(find.byType(VideoTile));
    }

    // A file sent with a 1×1 size, as sound-only MP4s once were.
    expect(await tileFor(1, 1), const Size(300, 300));
    expect(await tileFor(320, 240), const Size(400, 300));
    expect(await tileFor(1080, 1920), const Size(168.75, 300));
  });
}
