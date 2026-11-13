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
    // A named modal's root as the core sends it (`app.modals.<name>`
    // returns `ui.modal(props, children)`): the `modal` block itself, never
    // opened by an `open` prop. Fails if the layer draws that block as the
    // in-tree modal (nothing shows: the tickets example's New ticket sheet
    // was empty on the phone), on a sheet or on a card.
    for (final (width, shape) in [(360.0, 'sheet'), (800.0, 'card')]) {
      testWidgets('a_named_modal_shows_its_content_as_a_$shape', (tester) async {
        final controller = _showWithModals(
          nodes: [
            create(1, 'box', props: {'background': 'surface'}),
            create(21, 'text', props: {'text': 'Open New Ticket'}),
            create(20, 'column', children: [21]),
            create(
              2,
              'modal',
              props: {'size': 'medium', 'title': 'Tickets'},
              children: [20],
            ),
          ],
          main: 1,
          modals: [
            {
              'id': 2,
              'name': 'new_ticket',
              'size': 'medium',
              'dismissible': true,
            },
          ],
        );
        await pumpView(
          tester,
          controller,
          width: width,
          height: 500,
          window: Size(width, 600),
        );
        await tester.pumpAndSettle();
        expect(find.text('Open New Ticket'), findsOneWidget);
        expect(find.text('Tickets'), findsOneWidget);
        expect(tester.getSize(find.text('Open New Ticket')).height, greaterThan(0));
      });
    }

    // Fails if modal backdrop or card overflows the plugin area bounds.
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

    // Fails if multiple named modals do not stack in order with top modal on top.
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

    // Fails if an unset sheet modal does not adaptively render as a sheet at 400 wide and a card at 800 wide.
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
            'dismissible': true,
          },
        ],
      );

      // At 400 wide (compact < 600): draws as a bottom sheet (bottom == 300, width == 400).
      await pumpView(tester, controller, width: 400, height: 300);
      await tester.pumpAndSettle();

      final sheetFinder = find.byKey(
        const ValueKey<String>('modal_card_adaptive_modal'),
      );
      expect(sheetFinder, findsOneWidget);
      var rect = tester.getRect(sheetFinder);
      expect(rect.bottom, 300);
      expect(rect.width, 400);

      // At 800 wide (non-compact): draws as a centred card (width == 520, not pinned to bottom).
      await pumpView(tester, controller, width: 800, height: 600);
      await tester.pumpAndSettle();

      rect = tester.getRect(sheetFinder);
      expect(rect.width, 520);
      expect(rect.bottom, lessThan(600));
    });

    // Fails if an explicit sheet = false modal turns into a sheet at compact width.
    testWidgets('explicit_sheet_false_stays_card_at_compact_width', (
      tester,
    ) async {
      final controller = _showWithModals(
        nodes: [
          create(1, 'box'),
          create(2, 'text', props: {'text': 'Card Body'}),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'card_modal',
            'size': 'medium',
            'sheet': false,
            'dismissible': true,
          },
        ],
      );

      // At 400 wide (compact < 600) with sheet: false, it stays a centred card!
      await pumpView(tester, controller, width: 400, height: 300);
      await tester.pumpAndSettle();

      final cardFinder = find.byKey(
        const ValueKey<String>('modal_card_card_modal'),
      );
      expect(cardFinder, findsOneWidget);
      final rect = tester.getRect(cardFinder);
      expect(rect.bottom, lessThan(300));
    });

    // Fails if Escape key dismisses a non-dismissible modal or fails to dismiss a dismissible modal.
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

    // Fails if tabbing past the last control escapes the modal instead of cycling inside.
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

      final inside1Editable = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const ValueKey<int>(21)),
          matching: find.byType(EditableText),
        ),
      );

      // Press Tab again: should cycle back inside the modal to inside_input_1!
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();

      final outsideEditable = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const ValueKey<int>(10)),
          matching: find.byType(EditableText),
        ),
      );
      expect(inside1Editable.focusNode.hasFocus, isTrue);
      expect(outsideEditable.focusNode.hasFocus, isFalse);
    });

    // Fails if in-tree modal or sheet expands in-tree inside an unbounded container like column.
    testWidgets(
      'in_tree_modal_and_sheet_inside_column_do_not_cause_layout_errors',
      (tester) async {
        final controller = _showWithModals(
          nodes: [
            create(2, 'text', props: {'text': 'In-tree Modal Content'}),
            create(
              3,
              'modal',
              props: {'open': true, 'size': 'small'},
              children: [2],
            ),
            create(4, 'text', props: {'text': 'In-tree Sheet Content'}),
            create(5, 'sheet', props: {'open': true}, children: [4]),
            create(1, 'column', children: [3, 5]),
          ],
          main: 1,
        );

        await pumpView(tester, controller, width: 400, height: 400);
        await tester.pumpAndSettle();

        expect(tester.takeException(), isNull);
        expect(find.text('In-tree Modal Content'), findsOneWidget);
        expect(find.text('In-tree Sheet Content'), findsOneWidget);
      },
    );

    // Fails if in-tree modal size full does not fill the area width and height or small is not 360 px wide.
    testWidgets('in_tree_modal_sizes_and_full_size_fills_area', (tester) async {
      // 1. Small modal at 800 wide
      final smallController = _showWithModals(
        nodes: [
          create(2, 'text', props: {'text': 'Small Body'}),
          create(
            3,
            'modal',
            props: {'open': true, 'size': 'small', 'sheet': false},
            children: [2],
          ),
          create(1, 'box', children: [3]),
        ],
        main: 1,
      );

      await pumpView(tester, smallController, width: 800, height: 600);
      await tester.pumpAndSettle();

      final smallCard = find.byKey(
        const ValueKey<String>('in_tree_modal_card'),
      );
      expect(smallCard, findsOneWidget);
      expect(tester.getRect(smallCard).width, 360);

      // 2. Full size modal at 800x600
      final fullController = _showWithModals(
        nodes: [
          create(2, 'text', props: {'text': 'Full Body'}),
          create(
            3,
            'modal',
            props: {'open': true, 'size': 'full'},
            children: [2],
          ),
          create(1, 'box', children: [3]),
        ],
        main: 1,
      );

      await pumpView(tester, fullController, width: 800, height: 600);
      await tester.pumpAndSettle();

      final fullCard = find.byKey(const ValueKey<String>('in_tree_modal_card'));
      expect(fullCard, findsOneWidget);
      final fullRect = tester.getRect(fullCard);
      expect(fullRect.width, 800);
      expect(fullRect.height, 600);
    });

    // Fails if in-tree modal does not adapt to sheet at compact width or respect explicit sheet props.
    testWidgets('in_tree_modal_compact_and_explicit_sheet', (tester) async {
      // At 400 wide without sheet prop: adaptively turns into a bottom sheet!
      final adaptiveController = _showWithModals(
        nodes: [
          create(2, 'text', props: {'text': 'Adaptive Body'}),
          create(
            3,
            'modal',
            props: {'open': true, 'size': 'medium'},
            children: [2],
          ),
          create(1, 'box', children: [3]),
        ],
        main: 1,
      );

      await pumpView(tester, adaptiveController, width: 400, height: 300);
      await tester.pumpAndSettle();

      final cardFinder = find.byKey(
        const ValueKey<String>('in_tree_modal_card'),
      );
      expect(cardFinder, findsOneWidget);
      var rect = tester.getRect(cardFinder);
      expect(rect.bottom, 300);
      expect(rect.width, 400);

      // At 800 wide with sheet = true: explicitly draws as a bottom sheet
      final explicitSheetController = _showWithModals(
        nodes: [
          create(2, 'text', props: {'text': 'Explicit Sheet Body'}),
          create(
            3,
            'modal',
            props: {'open': true, 'size': 'medium', 'sheet': true},
            children: [2],
          ),
          create(1, 'box', children: [3]),
        ],
        main: 1,
      );

      await pumpView(tester, explicitSheetController, width: 800, height: 600);
      await tester.pumpAndSettle();

      final explicitCardFinder = find.byKey(
        const ValueKey<String>('in_tree_modal_card'),
      );
      rect = tester.getRect(explicitCardFinder);
      expect(rect.bottom, 600);
      expect(rect.width, 800);

      // At 400 wide with sheet = false: stays a centred card!
      final explicitCardController = _showWithModals(
        nodes: [
          create(2, 'text', props: {'text': 'Explicit Card Body'}),
          create(
            3,
            'modal',
            props: {'open': true, 'size': 'medium', 'sheet': false},
            children: [2],
          ),
          create(1, 'box', children: [3]),
        ],
        main: 1,
      );

      await pumpView(tester, explicitCardController, width: 400, height: 300);
      await tester.pumpAndSettle();

      final cardOnlyFinder = find.byKey(
        const ValueKey<String>('in_tree_modal_card'),
      );
      rect = tester.getRect(cardOnlyFinder);
      expect(rect.bottom, lessThan(300));
    });

    // Fails if in-tree modal title prop is not rendered above content in 18pt font.
    testWidgets('in_tree_modal_renders_title_above_content', (tester) async {
      final controller = _showWithModals(
        nodes: [
          create(2, 'text', props: {'text': 'Body Content'}),
          create(
            3,
            'modal',
            props: {'open': true, 'title': 'Modal Header Title'},
            children: [2],
          ),
          create(1, 'box', children: [3]),
        ],
        main: 1,
      );

      await pumpView(tester, controller, width: 800, height: 600);
      await tester.pumpAndSettle();

      final titleFinder = find.byKey(
        const ValueKey<String>('in_tree_modal_title'),
      );
      expect(titleFinder, findsOneWidget);
      final textWidget = tester.widget<Text>(titleFinder);
      expect(textWidget.data, 'Modal Header Title');
      expect(textWidget.style?.fontSize, 18);
      expect(textWidget.style?.fontWeight, FontWeight.w600);
    });

    // Fails if modal card stretches to full height instead of sizing to fit its content.
    testWidgets('modal_card_fits_content_height_and_scrolls_when_taller', (
      tester,
    ) async {
      final controller = _showWithModals(
        nodes: [
          create(1, 'box'),
          create(2, 'box', props: {'height': 80, 'width': 200}),
        ],
        main: 1,
        modals: [
          {
            'id': 2,
            'name': 'short_modal',
            'size': 'small',
            'sheet': false,
            'dismissible': true,
          },
        ],
      );

      await pumpView(tester, controller, width: 600, height: 800);
      await tester.pumpAndSettle();

      final cardFinder = find.byKey(
        const ValueKey<String>('modal_card_short_modal'),
      );
      final cardRect = tester.getRect(cardFinder);
      expect(cardRect.height, lessThan(200.0));
    });
  });

  group('dialogs and overlays', () {
    // Fails if quick confirm dialog does not send dialog answers for cancel and ok.
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

    // Fails if toast does not dismiss after duration seconds.
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

    // Fails if toast ignores open=false, draws in-place instead of bottom, or restarts timer on rebuild.
    testWidgets('toast_respects_open_and_does_not_restart_timer_on_rebuild', (
      tester,
    ) async {
      final controller = BlocksController();
      addTearDown(controller.dispose);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      controller.applyBatch(
        batch(1, [
          create(
            2,
            'toast',
            props: {'text': 'Notice', 'open': false, 'duration': 1.0},
            events: ['dismiss'],
          ),
          create(1, 'column', children: [2]),
          roots(1),
        ]),
      );

      await pumpView(tester, controller, width: 500, height: 400);
      await tester.pump();

      // open=false: should not draw
      expect(find.text('Notice'), findsNothing);

      // Open toast
      controller.applyBatch(
        batch(2, [
          {
            'op': 'props',
            'id': 2,
            'set': {'open': true},
            'unset': <String>[],
          },
        ]),
      );
      await tester.pump();
      await tester.pump();
      expect(find.text('Notice'), findsOneWidget);

      // Verify toast is positioned at bottom of 400 height area (bottom > 300)
      final toastRect = tester.getRect(find.text('Notice'));
      expect(toastRect.bottom, greaterThan(300.0));

      // Rebuild during 400ms (updating unrelated prop or re-rendering)
      await tester.pump(const Duration(milliseconds: 400));
      controller.applyBatch(
        batch(3, [
          {
            'op': 'props',
            'id': 2,
            'set': {'text': 'Notice Updated'},
            'unset': <String>[],
          },
        ]),
      );
      await tester.pump();

      // Advance 700ms more (total 1100ms > 1000ms duration)
      await tester.pump(const Duration(milliseconds: 700));

      // Timer must NOT have been reset by batch 3: dismiss event sent!
      final events = inputs.of('event');
      expect(events.length, 1);
      expect(events.first['event'], 'dismiss');
    });
  });
}
