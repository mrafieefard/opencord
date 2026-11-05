import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

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

Future<void> _pumpCompact(
  WidgetTester tester,
  Message message, {
  MarkdownContext? links,
  ReplyPreview? reply,
  VoidCallback? onRetry,
  VoidCallback? onReplyTap,
  ValueChanged<String>? onReaction,
  ValueChanged<String?>? onSelectionChanged,
}) => pumpThemed(
  tester,
  surface: const Size(800, 600),
  Align(
    alignment: Alignment.topLeft,
    child: CompactMessage(
      serverKey: 'test',
      message: message,
      first: true,
      author: 'Kai',
      reply: reply,
      links:
          links ??
          MarkdownContext(userName: (_) => 'Mira', channelName: (_) => 'help'),
      onRetry: onRetry,
      onReplyTap: onReplyTap,
      onReaction: onReaction,
      onSelectionChanged: onSelectionChanged,
    ),
  ),
);

void main() {
  for (final content in ['> quoted', '> quoted\nafter', '```\ncode\n```']) {
    testWidgets('a lead on its own line goes before: $content', (tester) async {
      await pumpThemed(
        tester,
        surface: const Size(800, 600),
        Align(
          alignment: Alignment.topLeft,
          child: MarkdownView(
            blocks: parsedContent(content),
            links: MarkdownContext(
              userName: (_) => 'Mira',
              channelName: (_) => 'help',
            ),
            color: const Color(0xFFFFFFFF),
            lead: const [TextSpan(text: 'Kai')],
          ),
        ),
      );

      expect(tester.takeException(), isNull);
      expect(find.textContaining('Kai', findRichText: true), findsOneWidget);
    });
  }

  testWidgets('selecting a message copies its mentions and code as text', (
    tester,
  ) async {
    String? selected;
    await _pumpCompact(
      tester,
      _message('hello <@7> see <#3> and `a code` ok'),
      links: MarkdownContext(
        userName: (_) => 'Mira',
        channelName: (_) => 'help',
      ),
      onSelectionChanged: (text) => selected = text,
    );

    tester
        .state<SelectableRegionState>(find.byType(SelectableRegion))
        .selectAll(SelectionChangedCause.keyboard);
    await tester.pump();

    expect(selected, 'hello @Mira see #help and a code ok');
  });

  testWidgets('code in a line leaves the line as tall as any other', (
    tester,
  ) async {
    Future<double> height(String content) async {
      await _pumpCompact(tester, _message(content));
      return tester.getSize(find.byType(CompactMessage)).height;
    }

    expect(await height('a `code` b'), await height('a code b'));
  });

  testWidgets('a reply shows its author in the secondary text colour', (
    tester,
  ) async {
    await _pumpCompact(
      tester,
      _message('Hi'),
      reply: const ReplyPreview(author: 'Mira', text: 'Ready?'),
    );

    final colors = tester.element(find.byType(CompactMessage)).oc;
    final richText = tester.widget<RichText>(
      find.descendant(
        of: find.byType(CompactMessage),
        matching: find.byWidgetPredicate(
          (w) => w is RichText && w.text.toPlainText().contains('Mira'),
        ),
      ),
    );
    final textSpan = richText.text as TextSpan;
    final innerSpan = textSpan.children!.first as TextSpan;
    final authorSpan = innerSpan.children!.first as TextSpan;
    expect(authorSpan.text, 'Mira');
    expect(authorSpan.style?.color, colors.textSecondary);
  });

  testWidgets('own messages say whether they are sending', (tester) async {
    final semantics = tester.ensureSemantics();
    await _pumpCompact(tester, _message('one', state: SendState.pending));
    expect(find.bySemanticsLabel(RegExp('Sending')), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('a failed send offers Retry', (tester) async {
    var retried = 0;
    await _pumpCompact(
      tester,
      _message('lost', state: SendState.failed),
      onRetry: () => retried++,
    );

    await tester.tap(find.text('Retry'));

    expect(retried, 1);
  });

  testWidgets('clicking a reaction toggles it', (tester) async {
    final toggled = <String>[];
    await _pumpCompact(
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
    await _pumpCompact(
      tester,
      _message('yes'),
      reply: const ReplyPreview(author: 'Mira', text: 'Ready?'),
      onReplyTap: () => jumped++,
    );

    await tester.tap(find.textContaining('Ready?', findRichText: true));

    expect(jumped, 1);
  });

  testWidgets('mentions and links are clickable', (tester) async {
    final users = <int>[];
    final channels = <int>[];
    final urls = <String>[];
    await _pumpCompact(
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
    await _pumpCompact(tester, _message('```dart\nprint(1);\n```'));

    await tester.tap(find.bySemanticsLabel('Copy code'));
    await tester.pump();

    expect(copied, 'print(1);');
    expect(find.text('Copied'), findsOneWidget);
    await tester.pump(const Duration(seconds: 3));
  });
}
