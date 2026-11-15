import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_plugin_host/opencord_plugin_host.dart';

const _style = TextStyle(fontFamily: 'Inter', fontSize: 14, color: Color(0xFFE6E6EA));
const _link = Color(0xFF7C8CFF);
const _code = Color(0xFF26262C);

Future<void> _pump(
  WidgetTester tester,
  String markdown, {
  String? Function(int id)? userName,
  String? Function(int id)? channelName,
  void Function(String url)? onLink,
  Brightness brightness = Brightness.dark,
  bool underlineLinks = false,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: ThemeData(brightness: brightness),
      home: Scaffold(
        body: Padding(
          padding: const EdgeInsets.all(12),
          child: MarkdownText(
            markdown: markdown,
            style: brightness == Brightness.dark
                ? _style
                : _style.copyWith(color: const Color(0xFF1C1C21)),
            linkColor: _link,
            codeBackground: brightness == Brightness.dark
                ? _code
                : const Color(0xFFEDEDF0),
            userName: userName,
            channelName: channelName,
            onLink: onLink,
            underlineLinks: underlineLinks,
          ),
        ),
      ),
    ),
  );
}

/// Every text span drawn, with the style it inherits.
List<(String, TextStyle)> _spans(WidgetTester tester) {
  final found = <(String, TextStyle)>[];
  void visit(InlineSpan span, TextStyle inherited) {
    final style = inherited.merge(span.style);
    if (span is TextSpan) {
      if (span.text != null) found.add((span.text!, style));
      for (final child in span.children ?? const <InlineSpan>[]) {
        visit(child, style);
      }
    }
  }

  for (final text in tester.widgetList<RichText>(find.byType(RichText))) {
    visit(text.text, const TextStyle());
  }
  return found;
}

TextStyle _styleOf(WidgetTester tester, String text) =>
    _spans(tester).firstWhere((span) => span.$1 == text).$2;

void main() {
  // Fails if a paragraph's text isn't drawn.
  testWidgets('a paragraph is drawn', (tester) async {
    await _pump(tester, 'Hello there');
    expect(find.textContaining('Hello there', findRichText: true), findsOneWidget);
  });

  // Fails if bold, italic or strike lose their style.
  testWidgets('bold, italic and strikethrough are styled', (tester) async {
    await _pump(tester, '**loud** *soft* ~~gone~~');
    expect(_styleOf(tester, 'loud').fontWeight, FontWeight.w700);
    expect(_styleOf(tester, 'soft').fontStyle, FontStyle.italic);
    expect(_styleOf(tester, 'gone').decoration, TextDecoration.lineThrough);
  });

  // Fails if inline code isn't monospace on the code background.
  testWidgets('inline code is monospace on the code background', (tester) async {
    await _pump(tester, 'run `make all` now');
    final style = _styleOf(tester, 'make all');
    expect(style.fontFamily, 'JetBrainsMono');
    expect(style.backgroundColor, _code);
  });

  // Fails if a fenced block isn't drawn monospace in a box of its own.
  testWidgets('a fenced code block is monospace in its own box', (tester) async {
    await _pump(tester, 'Before\n```dart\nfinal x = 1;\n```');
    expect(_styleOf(tester, 'final x = 1;').fontFamily, 'JetBrainsMono');
    final box = tester.widget<DecoratedBox>(
      find.ancestor(
        of: find.textContaining('final x = 1;', findRichText: true),
        matching: find.byType(DecoratedBox),
      ).first,
    );
    expect((box.decoration as BoxDecoration).color, _code);
  });

  // Fails if a quote loses its bar on the left.
  testWidgets('a quote has a bar on its left', (tester) async {
    await _pump(tester, '> wise words');
    final box = tester.widget<DecoratedBox>(
      find.ancestor(
        of: find.textContaining('wise words', findRichText: true),
        matching: find.byType(DecoratedBox),
      ).first,
    );
    final border = (box.decoration as BoxDecoration).border! as Border;
    expect(border.left.width, greaterThan(0));
  });

  // Fails if a link isn't the link colour, or a tap doesn't reach onLink
  // with its url.
  // Fails if a phone's link isn't underlined (its accent can be the text's
  // colour, so it looks like plain text), or the desktop's now is.
  testWidgets('links are underlined only when asked', (tester) async {
    await _pump(tester, 'see https://opencord.dev ok');
    expect(_styleOf(tester, 'https://opencord.dev').decoration, isNot(TextDecoration.underline));
    await _pump(tester, 'see https://opencord.dev ok', underlineLinks: true);
    expect(_styleOf(tester, 'https://opencord.dev').decoration, TextDecoration.underline);
  });

  testWidgets('tapping a link calls onLink with its url', (tester) async {
    final opened = <String>[];
    await _pump(tester, 'see https://opencord.dev/docs ok', onLink: opened.add);
    expect(_styleOf(tester, 'https://opencord.dev/docs').color, _link);
    final span = _spans(tester);
    expect(span.any((s) => s.$1 == 'https://opencord.dev/docs'), isTrue);
    final rich = tester.widgetList<RichText>(find.byType(RichText)).first;
    TapGestureRecognizer? recognizer;
    rich.text.visitChildren((child) {
      if (child is TextSpan &&
          child.text == 'https://opencord.dev/docs' &&
          child.recognizer is TapGestureRecognizer) {
        recognizer = child.recognizer! as TapGestureRecognizer;
      }
      return true;
    });
    recognizer!.onTap!();
    expect(opened, ['https://opencord.dev/docs']);
  });

  // Fails if a mention shows its raw token, or a name the callback didn't
  // give.
  testWidgets('mentions show names, and unknown when there is none', (tester) async {
    await _pump(
      tester,
      'hi <@42> and <@7> in <#5> and <#6>',
      userName: (id) => id == 42 ? 'Ava' : null,
      channelName: (id) => id == 5 ? 'general' : null,
    );
    final text = _spans(tester).map((s) => s.$1).join();
    expect(text, contains('@Ava'));
    expect(text, contains('@unknown'));
    expect(text, contains('#general'));
    expect(text, contains('#unknown'));
    expect(text, isNot(contains('<@')));
  });

  for (final brightness in Brightness.values) {
    testWidgets('one message with every construct (${brightness.name})', (tester) async {
      tester.view.physicalSize = const Size(400, 360);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await _pump(
        tester,
        'Hello **bold** *italic* ~~strike~~ `code` https://opencord.dev <@42> <#5>\n'
        '> a quote\n'
        '```\nfn main() {}\n```\n'
        'After the code.',
        userName: (id) => 'Ava',
        channelName: (id) => 'general',
        brightness: brightness,
      );
      await expectLater(
        find.byType(MarkdownText),
        matchesGoldenFile('goldens/markdown_text_${brightness.name}.png'),
      );
    });
  }
}
