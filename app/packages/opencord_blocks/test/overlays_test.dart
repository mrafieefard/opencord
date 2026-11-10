import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

BlocksController _showWithModals({
  required List<Map<String, Object?>> nodes,
  required int main,
  List<Map<String, Object?>> modals = const [],
}) {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  controller.applyBatch(
    batch(1, [
      ...nodes,
      {'op': 'roots', 'main': main, 'modals': modals},
    ]),
  );
  return controller;
}

void main() {
  group('modals and sheets', () {
    testWidgets('a_modal_stays_inside_the_area', (tester) async {
      final controller = _showWithModals(
        nodes: [
          create(1, 'box', props: {'background': 'surface'}),
          create(
            2,
            'box',
            key: 'modal_content',
            props: {'text': 'Inside Modal'},
          ),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'test_modal',
            'size': 'large',
            'sheet': false,
            'dismissible': true,
          },
        ],
      );

      // Area is 400 × 300 placed inside an 800 × 600 window.
      await pumpView(
        tester,
        controller,
        width: 400,
        height: 300,
        window: const Size(800, 600),
      );
      await tester.pumpAndSettle();

      final modalFinder = find.byKey(
        const ValueKey<String>('modal_backdrop_test_modal'),
      );
      expect(modalFinder, findsOneWidget);
      final modalRect = tester.getRect(modalFinder);

      // Area sits at (0, 0) of width 400 and height 300.
      expect(modalRect.left, greaterThanOrEqualTo(0));
      expect(modalRect.top, greaterThanOrEqualTo(0));
      expect(modalRect.right, lessThanOrEqualTo(400.01));
      expect(modalRect.bottom, lessThanOrEqualTo(300.01));

      // Modal card must also stay within bounds (not overflow 400 width).
      final cardFinder = find.byKey(
        const ValueKey<String>('modal_card_test_modal'),
      );
      expect(cardFinder, findsOneWidget);
      final cardRect = tester.getRect(cardFinder);
      expect(cardRect.left, greaterThanOrEqualTo(0));
      expect(cardRect.right, lessThanOrEqualTo(400.01));
    });

    testWidgets('named_modals_stack_in_order', (tester) async {
      final controller = _showWithModals(
        nodes: [
          create(1, 'box'),
          create(2, 'text', props: {'text': 'Modal Bottom'}),
          create(3, 'text', props: {'text': 'Modal Top'}),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'bottom_modal',
            'size': 'small',
            'sheet': false,
            'dismissible': true,
          },
          {
            'id': 3,
            'name': 'top_modal',
            'size': 'small',
            'sheet': false,
            'dismissible': true,
          },
        ],
      );

      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pumpAndSettle();

      // Both modals are drawn, with top_modal being rendered after bottom_modal.
      expect(find.text('Modal Bottom'), findsOneWidget);
      expect(find.text('Modal Top'), findsOneWidget);

      final bottomBackdrop = tester.getRect(
        find.byKey(const ValueKey<String>('modal_backdrop_bottom_modal')),
      );
      final topBackdrop = tester.getRect(
        find.byKey(const ValueKey<String>('modal_backdrop_top_modal')),
      );
      expect(bottomBackdrop, equals(topBackdrop));
    });

    testWidgets('compact_turns_a_modal_into_a_sheet', (tester) async {
      final controller = _showWithModals(
        nodes: [
          create(1, 'box'),
          create(2, 'text', props: {'text': 'Modal Body'}),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'adaptive_modal',
            'size': 'medium',
            'sheet': true,
            'dismissible': true,
          },
        ],
      );

      // Compact width < 600 px (e.g. 500 px width, 400 px height).
      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pumpAndSettle();

      final sheetFinder = find.byKey(
        const ValueKey<String>('modal_card_adaptive_modal'),
      );
      expect(sheetFinder, findsOneWidget);
      final sheetRect = tester.getRect(sheetFinder);

      // In sheet mode, the card aligns to the bottom of the area (bottom == 400).
      expect(sheetRect.bottom, 400);
      expect(sheetRect.width, 500); // stretches full width in sheet mode
    });

    testWidgets('esc_closes_only_dismissible_ones', (tester) async {
      final controller = _showWithModals(
        nodes: [
          create(1, 'box'),
          create(2, 'text', props: {'text': 'Locked Modal'}),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'locked',
            'size': 'small',
            'sheet': false,
            'dismissible': false,
          },
        ],
      );
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pumpAndSettle();

      // Press Esc on locked modal
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      // No modal_dismiss sent!
      expect(inputs.of('modal_dismiss'), isEmpty);

      // Now update to dismissible
      controller.applyBatch(
        batch(2, [
          {
            'op': 'roots',
            'main': 1,
            'modals': [
              {
                'id': 2,
                'name': 'locked',
                'size': 'small',
                'sheet': false,
                'dismissible': true,
              },
            ],
          },
        ]),
      );
      await tester.pumpAndSettle();

      // Press Esc on dismissible modal
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      final dismisses = inputs.of('modal_dismiss');
      expect(dismisses.length, 1);
      expect(dismisses.first['name'], 'locked');
    });

    testWidgets('focus_stays_in_the_top_modal', (tester) async {
      final controller = _showWithModals(
        nodes: [
          create(
            10,
            'text_input',
            key: 'outside_input',
            props: {'value': 'Outside'},
          ),
          create(1, 'column', children: [10]),
          create(
            21,
            'text_input',
            key: 'inside_input_1',
            props: {'value': 'Inside 1'},
          ),
          create(
            22,
            'text_input',
            key: 'inside_input_2',
            props: {'value': 'Inside 2'},
          ),
          create(2, 'column', children: [21, 22]),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'focused_modal',
            'size': 'medium',
            'sheet': false,
            'dismissible': true,
          },
        ],
      );

      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pumpAndSettle();

      // Tap on inside_input_1 to focus it
      await tester.tap(find.text('Inside 1'));
      await tester.pumpAndSettle();

      // Press Tab to advance focus
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();

      // Inside input 2 should now have focus
      final inside2Editable = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const ValueKey<int>(22)),
          matching: find.byType(EditableText),
        ),
      );
      expect(inside2Editable.focusNode.hasFocus, isTrue);

      // Press Tab again: should cycle back inside the modal, NOT escape to outside input!
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();

      final outsideEditable = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const ValueKey<int>(10)),
          matching: find.byType(EditableText),
        ),
      );
      expect(outsideEditable.focusNode.hasFocus, isFalse);
    });
  });

  group('dialogs and overlays', () {
    testWidgets('a_confirm_answers_true_and_false', (tester) async {
      final controller = BlocksController();
      addTearDown(controller.dispose);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      // Create main node and dialog
      controller.applyBatch(
        batch(1, [
          create(1, 'box'),
          roots(1),
          {
            'op': 'dialog',
            'id': 42,
            'kind': 'confirm',
            'text': 'Delete this file?',
          },
        ]),
      );

      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pumpAndSettle();

      expect(find.text('Delete this file?'), findsOneWidget);
      expect(find.text('Cancel'), findsOneWidget);
      expect(find.text('OK'), findsOneWidget);

      // Tap Cancel
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      final cancelAnswers = inputs.of('dialog');
      expect(cancelAnswers.length, 1);
      expect(cancelAnswers.first['id'], 42);
      expect(cancelAnswers.first['result'], false);

      // Now test OK with a new dialog
      controller.applyBatch(
        batch(2, [
          {
            'op': 'dialog',
            'id': 43,
            'kind': 'confirm',
            'text': 'Save changes?',
          },
        ]),
      );
      await tester.pumpAndSettle();

      expect(find.text('Save changes?'), findsOneWidget);
      await tester.tap(find.text('OK'));
      await tester.pumpAndSettle();

      final okAnswers = inputs.of('dialog');
      expect(okAnswers.length, 2);
      expect(okAnswers.last['id'], 43);
      expect(okAnswers.last['result'], true);
    });

    testWidgets('a_toast_dismisses_after_its_duration', (tester) async {
      final controller = BlocksController();
      addTearDown(controller.dispose);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      controller.applyBatch(
        batch(1, [
          create(
            2,
            'toast',
            props: {'text': 'File saved!', 'duration': 1.0},
            events: ['dismiss'],
          ),
          create(1, 'column', children: [2]),
          roots(1),
        ]),
      );

      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pump();

      expect(find.text('File saved!'), findsOneWidget);
      expect(inputs.of('event'), isEmpty);

      // Advance 500 ms (halfway)
      await tester.pump(const Duration(milliseconds: 500));
      expect(find.text('File saved!'), findsOneWidget);
      expect(inputs.of('event'), isEmpty);

      // Advance another 600 ms (past 1.0 s)
      await tester.pump(const Duration(milliseconds: 600));

      final events = inputs.of('event');
      expect(events.length, 1);
      expect(events.first['event'], 'dismiss');
    });
  });
}
