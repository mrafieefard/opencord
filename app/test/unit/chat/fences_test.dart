import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/fences.dart';

void main() {
  group('findFences', () {
    test('table row 1: language and code with blank lines at ends', () {
      final fences = findFences('```py\n\nCODE```');
      expect(fences, [
        const Fence(start: 0, end: 14, language: 'py', code: 'CODE'),
      ]);
    });

    test('table row 2: language and code', () {
      final fences = findFences('```js\nCode\n```');
      expect(fences, [
        const Fence(start: 0, end: 14, language: 'js', code: 'Code'),
      ]);
    });

    test('table row 3: one line without language', () {
      final fences = findFences('```one line```');
      expect(fences, [
        const Fence(start: 0, end: 14, language: null, code: 'one line'),
      ]);
    });

    test('table row 4: one line with space, no language', () {
      final fences = findFences('```py code```');
      expect(fences, [
        const Fence(start: 0, end: 13, language: null, code: 'py code'),
      ]);
    });

    test('table row 5: newline then closing fence and trailing text', () {
      final fences = findFences('```\nb``` after');
      expect(fences, [
        const Fence(start: 0, end: 8, language: null, code: 'b'),
      ]);
    });

    test('table row 6: unclosed fence is not a block', () {
      final fences = findFences('```\nno end');
      expect(fences, isEmpty);
    });

    test('a_language_can_have_these_characters', () {
      expect(findFences('```c++\ncode```').single.language, 'c++');
      expect(findFences('```c#\ncode```').single.language, 'c#');
      expect(
        findFences('```objective-c\ncode```').single.language,
        'objective-c',
      );
      expect(findFences('```f.x\ncode```').single.language, 'f.x');

      // 33 characters isn't a language (spec: [A-Za-z0-9_+#.-]{1,32})
      final longLang = 'a' * 33;
      final fence33 = findFences('```$longLang\ncode```').single;
      expect(fence33.language, isNull);
      expect(fence33.code, '$longLang\ncode');
    });

    test('indentation_and_blank_lines_inside_stay', () {
      const input = '```py\n  def foo():\n\n    return 42\n```';
      final fence = findFences(input).single;
      expect(fence.language, 'py');
      expect(fence.code, '  def foo():\n\n    return 42');
    });

    test('blank_lines_at_the_ends_go', () {
      const input = '```py\n\n\ncode\n\n\n```';
      final fence = findFences(input).single;
      expect(fence.language, 'py');
      expect(fence.code, 'code');
    });

    test('an_opening_fence_mid_line_is_not_a_block', () {
      expect(findFences('see ```x``` here'), isEmpty);
    });

    test('two_blocks_in_one_message', () {
      const input = '```py\nx = 1\n```\nmiddle\n```js\ny = 2\n```';
      final fences = findFences(input);
      expect(fences, hasLength(2));
      expect(fences[0].code, 'x = 1');
      expect(fences[0].language, 'py');
      expect(fences[1].code, 'y = 2');
      expect(fences[1].language, 'js');
    });

    test('empty_fences_stay_text', () {
      expect(findFences('``````'), isEmpty);
      expect(findFences('```\n```'), isEmpty);
      expect(findFences('```   ```'), isEmpty);
    });

    test('text_before_and_after_a_block', () {
      const input = 'before\n```py\nx = 1\n```\nafter';
      final fences = findFences(input);
      expect(fences, hasLength(1));
      expect(fences.single.start, 7);
      expect(fences.single.end, 22);
      expect(fences.single.code, 'x = 1');
    });
  });

  group('endsInsideFence', () {
    test('open fence with language', () {
      expect(endsInsideFence('```py'), isTrue);
      expect(endsInsideFence('```py\n'), isTrue);
      expect(endsInsideFence('```py\nx = 1'), isTrue);
    });

    test('open fence with indentation', () {
      expect(endsInsideFence('  ```py\nx = 1'), isTrue);
    });

    test('closed fence is not open', () {
      expect(endsInsideFence('```py\nx = 1\n```'), isFalse);
      expect(endsInsideFence('```one line```'), isFalse);
    });

    test('the_cursor_decides', () {
      // Cursor before opening fence
      expect(endsInsideFence(''), isFalse);
      expect(endsInsideFence('hello\n'), isFalse);
    });

    test('mid-line fence is not open fence', () {
      expect(endsInsideFence('see ```x'), isFalse);
    });
  });
}
