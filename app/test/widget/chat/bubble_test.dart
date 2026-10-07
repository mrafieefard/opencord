import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/markdown_view.dart';

import '../../support/pump.dart';

final _at = DateTime(2026, 10, 7, 9, 41);

Message _message(
  String content, {
  SendState state = SendState.sent,
  List<Reaction> reactions = const [],
}) => Message(
  id: 1,
  channelId: 10,
  authorId: 2,
  content: content,
  createdAt: _at,
  reactions: reactions,
  sendState: state,
);

Future<void> _pumpBubble(
  WidgetTester tester,
  Message message, {
  bool own = false,
  MarkdownContext? links,
  ReplyPreview? reply,
  VoidCallback? onRetry,
  VoidCallback? onReplyTap,
  ValueChanged<String>? onReaction,
}) => pumpThemed(
  tester,
  surface: const Size(800, 600),
  Align(
    alignment: Alignment.topLeft,
    child: MessageBubble(
      message: message,
      own: own,
      first: true,
      last: true,
      author: 'Kai',
      reply: reply,
      links:
          links ??
          MarkdownContext(userName: (_) => 'Mira', channelName: (_) => 'help'),
      onRetry: onRetry,
      onReplyTap: onReplyTap,
      onReaction: onReaction,
    ),
  ),
);

Rect _rectOf(WidgetTester tester, String text) =>
    tester.getRect(find.textContaining(text, findRichText: true).first);

void main() {
  testWidgets('a short message gets a short bubble with the time inline', (
    tester,
  ) async {
    // Own, so there is no author line above the text.
    await _pumpBubble(tester, _message('Hi'), own: true);

    final bubble = tester.getRect(find.byType(MessageBubble));
    final text = tester.getRect(find.textContaining('Hi', findRichText: true));
    final time = tester.getRect(find.text('09:41'));

    expect(bubble.width, lessThan(120));
    expect(bubble.height, lessThan(40));
    expect(time.center.dy, inInclusiveRange(text.top, text.bottom));
  });

  testWidgets('the time never covers the text, wrapping when it must', (
    tester,
  ) async {
    var wrapped = 0;
    for (var words = 1; words <= 40; words++) {
      await _pumpBubble(
        tester,
        _message(List.filled(words, 'word').join(' ')),
        own: true,
      );
      final paragraph = tester.renderObject<RenderParagraph>(
        find.textContaining('word', findRichText: true),
      );
      final end = paragraph.text.toPlainText().lastIndexOf('word');
      final box = paragraph
          .getBoxesForSelection(
            TextSelection(baseOffset: end, extentOffset: end + 4),
          )
          .last
          .toRect()
          .shift(paragraph.localToGlobal(Offset.zero));
      final time = tester.getRect(find.text('09:41'));
      final bubble = tester.getRect(find.byType(MessageBubble));

      expect(time.overlaps(box), isFalse, reason: '$words words');
      expect(bubble.contains(time.bottomRight), isTrue, reason: '$words words');
      if (time.top >= box.bottom - 1) wrapped++;
    }
    // Some lengths filled the last line, so the time took a line of its own.
    expect(wrapped, greaterThan(0));
  });

  testWidgets('a message ending in code puts the time on its own line', (
    tester,
  ) async {
    await _pumpBubble(tester, _message('```\nlet x = 1;\n```'));

    final code = _rectOf(tester, 'let x = 1;');
    final time = _rectOf(tester, '09:41');

    expect(time.top, greaterThan(code.bottom));
  });

  testWidgets('own messages say whether they were delivered', (tester) async {
    final semantics = tester.ensureSemantics();
    await _pumpBubble(
      tester,
      _message('one', state: SendState.pending),
      own: true,
    );
    expect(find.bySemanticsLabel(RegExp('Sending')), findsOneWidget);

    await _pumpBubble(tester, _message('one'), own: true);
    expect(find.bySemanticsLabel(RegExp('Delivered')), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('a failed send offers Retry', (tester) async {
    var retried = 0;
    await _pumpBubble(
      tester,
      _message('lost', state: SendState.failed),
      own: true,
      onRetry: () => retried++,
    );

    await tester.tap(find.text('Retry'));

    expect(retried, 1);
  });

  testWidgets('clicking a reaction toggles it', (tester) async {
    final toggled = <String>[];
    await _pumpBubble(
      tester,
      _message(
        'vote',
        reactions: const [
          Reaction(emoji: '👍', userIds: [1, 2], me: true),
          Reaction(emoji: '🎉', userIds: [3], me: false),
        ],
      ),
      onReaction: toggled.add,
    );

    await tester.tap(find.text('🎉'));

    expect(toggled, ['🎉']);
    expect(find.text('2'), findsOneWidget);
  });

  testWidgets('clicking the reply quote jumps to the original', (tester) async {
    var jumped = 0;
    await _pumpBubble(
      tester,
      _message('yes'),
      reply: const ReplyPreview(author: 'Mira', text: 'Ready?'),
      onReplyTap: () => jumped++,
    );

    await tester.tap(find.text('Ready?'));

    expect(jumped, 1);
  });

  testWidgets('mentions and links are clickable', (tester) async {
    final users = <int>[];
    final channels = <int>[];
    final urls = <String>[];
    await _pumpBubble(
      tester,
      _message('<@7> see <#9> https://example.org'),
      links: MarkdownContext(
        userName: (_) => 'Mira',
        channelName: (_) => 'help',
        onUser: users.add,
        onChannel: channels.add,
        onLink: urls.add,
      ),
    );

    for (final text in ['@Mira', '#help', 'https://example.org']) {
      await tester.tapOnText(find.textRange.ofSubstring(text));
    }

    expect(users, [7]);
    expect(channels, [9]);
    expect(urls, ['https://example.org']);
  });

  testWidgets('the copy button copies the code block', (tester) async {
    String? copied;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied = (call.arguments as Map)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    await _pumpBubble(tester, _message('```dart\nprint(1);\n```'));

    await tester.tap(find.bySemanticsLabel('Copy code'));
    await tester.pump();

    expect(copied, 'print(1);');
    expect(find.text('Copied'), findsOneWidget);
    await tester.pump(const Duration(seconds: 3));
  });
}
