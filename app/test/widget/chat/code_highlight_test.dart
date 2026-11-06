import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/code_highlight.dart';
import 'package:opencord/features/chat/markdown.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

import '../../support/clipboard.dart';

List<TextSpan> _flatten(InlineSpan span) {
  final result = <TextSpan>[];
  void visit(InlineSpan s) {
    if (s is TextSpan) {
      if (s.text != null && s.text!.isNotEmpty) {
        result.add(s);
      }
      if (s.children != null) {
        for (final child in s.children!) {
          visit(child);
        }
      }
    }
  }

  visit(span);
  return result;
}

void main() {
  const dark = OcColors.dark;
  const light = OcColors.light;

  test('code_is_highlighted_for_its_language', () {
    const code = 'def greet(name):\n    # say hello\n    return "hi"';
    final span = highlightedCode(code, 'python', dark);
    expect(span, isNotNull);

    final leaves = _flatten(span!);
    final defSpan = leaves.firstWhere((s) => s.text == 'def');
    expect(defSpan.style?.color, dark.text);
    expect(defSpan.style?.fontWeight, FontWeight.w700);

    final strSpan = leaves.firstWhere((s) => s.text == '"hi"');
    expect(strSpan.style?.color, dark.textSecondary);

    final commentSpan = leaves.firstWhere((s) => s.text == '# say hello');
    expect(commentSpan.style?.color, dark.textMuted);

    final nameSpan = leaves.firstWhere((s) => s.text == 'name');
    expect(nameSpan.style?.color, dark.text);
  });

  test('function_names_take_title', () {
    const code = 'def greet(name):\n    pass';
    final span = highlightedCode(code, 'python', dark);
    expect(span, isNotNull);

    final leaves = _flatten(span!);
    final titleSpan = leaves.firstWhere((s) => s.text == 'greet');
    expect(titleSpan.style?.color, dark.text);
    expect(titleSpan.style?.fontWeight, FontWeight.w500);
  });

  test('every_alias_works', () {
    final pairs = [
      ('python', 'py', 'def f(): pass'),
      ('javascript', 'js', 'const x = 1;'),
      ('javascript', 'jsx', '<div />'),
      ('typescript', 'ts', 'const x: number = 1;'),
      ('typescript', 'tsx', 'const el: JSX.Element = <div />;'),
      ('rust', 'rs', 'fn main() {}'),
      ('go', 'golang', 'package main'),
      ('kotlin', 'kt', 'fun main() {}'),
      ('ruby', 'rb', 'def hello; end'),
      ('yaml', 'yml', 'key: value'),
      ('markdown', 'md', '# Title'),
      ('c', 'h', 'int foo();'),
      ('cpp', 'c++', 'int main() {}'),
      ('csharp', 'cs', 'class Foo {}'),
      ('bash', 'sh', 'echo hello'),
      ('bash', 'zsh', 'echo hello'),
      ('xml', 'html', '<html></html>'),
      ('ini', 'toml', '[section]\nkey = "val"'),
    ];

    for (final (canonical, alias, sample) in pairs) {
      final spanCanonical = highlightedCode(sample, canonical, dark);
      final spanAlias = highlightedCode(sample, alias, dark);
      expect(
        spanCanonical,
        isNotNull,
        reason: 'canonical $canonical failed for $sample',
      );
      expect(spanAlias, isNotNull, reason: 'alias $alias failed for $sample');

      final leavesCanonical = _flatten(spanCanonical!);
      final leavesAlias = _flatten(spanAlias!);
      expect(leavesAlias.length, leavesCanonical.length);
      for (var i = 0; i < leavesCanonical.length; i++) {
        expect(leavesAlias[i].text, leavesCanonical[i].text);
        expect(leavesAlias[i].style?.color, leavesCanonical[i].style?.color);
        expect(
          leavesAlias[i].style?.fontWeight,
          leavesCanonical[i].style?.fontWeight,
        );
      }
    }
  });

  test('an_unknown_language_stays_plain', () {
    expect(highlightedCode('x = 1', 'unknown_lang', dark), isNull);
  });

  test('no_language_stays_plain', () {
    expect(highlightedCode('x = 1', null, dark), isNull);
    expect(highlightedCode('x = 1', '', dark), isNull);
  });

  test('a_huge_block_stays_plain', () {
    final huge = 'a' * 20001;
    expect(highlightedCode(huge, 'python', dark), isNull);
  });

  test('its_second_call_is_cached', () {
    const code = 'const cached = true;';
    final first = highlightedCode(code, 'javascript', dark);
    final second = highlightedCode(code, 'javascript', dark);
    expect(identical(first, second), isTrue);

    // After 64 other blocks, the 65th drops the oldest
    for (var i = 0; i < 65; i++) {
      highlightedCode('var x$i = $i;', 'javascript', dark);
    }
    final afterEviction = highlightedCode(code, 'javascript', dark);
    expect(identical(first, afterEviction), isFalse);
  });

  test('a_theme_change_rebuilds_it', () {
    const code = 'let x = 10;';
    final darkSpan = highlightedCode(code, 'rust', dark);
    final lightSpan = highlightedCode(code, 'rust', light);
    expect(identical(darkSpan, lightSpan), isFalse);

    final darkLeaves = _flatten(darkSpan!);
    final lightLeaves = _flatten(lightSpan!);
    final darkNum = darkLeaves.firstWhere((s) => s.text == '10');
    final lightNum = lightLeaves.firstWhere((s) => s.text == '10');
    expect(darkNum.style?.color, dark.textSecondary);
    expect(lightNum.style?.color, light.textSecondary);
  });

  test('only_grey', () {
    const code =
        'def solve(n: int) -> str:\n    # comment\n    return f"res: {n * 2}"';
    final span = highlightedCode(code, 'python', dark)!;
    final leaves = _flatten(span);

    final allowedColors = {dark.text, dark.textSecondary, dark.textMuted};

    for (final leaf in leaves) {
      final color = leaf.style?.color;
      expect(
        allowedColors.contains(color),
        isTrue,
        reason: 'leaf "${leaf.text}" has non-grey color $color',
      );
    }
  });

  testWidgets('copy_gives_what_was_typed and the_label_shows_what_was_typed', (
    tester,
  ) async {
    final clipboard = ClipboardSpy(tester);
    const typedCode = '  def test():\n    return 42';
    const typedLang = 'py';

    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(dark),
        home: Scaffold(
          body: MarkdownView(
            blocks: const [MdCode(typedCode, language: typedLang)],
            links: MarkdownContext(
              userName: (_) => null,
              channelName: (_) => null,
            ),
            color: dark.text,
          ),
        ),
      ),
    );

    // The label shows what was typed: 'py'
    expect(find.text('py'), findsOneWidget);

    // Copy button gives the code as typed
    final copyButton = find.byType(Hoverable);
    expect(copyButton, findsOneWidget);
    await tester.tap(copyButton);
    await tester.pump();

    expect(clipboard.copied, typedCode);
    // Pump past the toast duration to leave no pending timers.
    await tester.pump(const Duration(seconds: 4));
  });
}
