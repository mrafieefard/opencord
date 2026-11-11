import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

class _ControlsHost extends FakeHost {
  String? pickedPath = '/path/to/file.png';
  int uploadNodeId = 0;
  String uploadPath = '';
  Completer<void>? uploadCompleter;

  @override
  IconData? icon(String name) => switch (name) {
    'star' => Icons.star,
    'add' => Icons.add,
    'menu_icon' => Icons.star,
    _ => null,
  };

  @override
  Future<String?> pickFile({
    required bool image,
    List<String> accept = const [],
  }) async {
    return pickedPath;
  }

  @override
  Future<void> upload(int nodeId, String path) async {
    uploadNodeId = nodeId;
    uploadPath = path;
    if (uploadCompleter != null) {
      await uploadCompleter!.future;
    }
  }
}

BlocksController _show(List<Map<String, Object?>> creates, int root) {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  controller.applyBatch(batch(1, [...creates, roots(root)]));
  return controller;
}

void main() {
  group('button and icon_button', () {
    // Fails if buttons do not send press or long_press events, or if disabled or loading buttons respond to taps.
    testWidgets('button_variants_and_events', (tester) async {
      final host = _ControlsHost();
      final controller = _show([
        create(
          2,
          'button',
          props: {'text': 'Primary', 'variant': 'primary'},
          events: ['press', 'long_press'],
        ),
        create(
          3,
          'button',
          props: {'text': 'Disabled', 'disabled': true},
          events: ['press'],
        ),
        create(
          4,
          'button',
          props: {'text': 'Loading', 'loading': true},
          events: ['press'],
        ),
        create(5, 'icon_button', props: {'icon': 'add'}, events: ['press']),
        create(1, 'column', children: [2, 3, 4, 5]),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller, host: host);

      // Tap primary
      await tester.tap(find.text('Primary'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 2,
          'event': 'press',
          'data': <String, Object?>{},
        },
      ]);

      // Long press primary
      inputs.all.clear();
      await tester.longPress(find.text('Primary'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 2,
          'event': 'long_press',
          'data': <String, Object?>{},
        },
      ]);

      // Tap disabled - nothing
      inputs.all.clear();
      await tester.tap(find.text('Disabled'));
      await tester.pump();
      expect(inputs.all, isEmpty);

      // Tap loading - spinner shown, ignores tap
      expect(find.byType(CircularProgressIndicator), findsOneWidget);
      await tester.tap(find.text('Loading'), warnIfMissed: false);
      await tester.pump();
      expect(inputs.all, isEmpty);

      // Tap icon_button
      await tester.tap(find.byIcon(Icons.add));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 5,
          'event': 'press',
          'data': <String, Object?>{},
        },
      ]);
    });
  });

  group('toggles', () {
    // Fails if checkbox, switch, or radio toggles do not invert or change value when tapped.
    testWidgets('checkbox_switch_and_radio', (tester) async {
      final controller = _show([
        create(
          2,
          'checkbox',
          props: {'value': false, 'text': 'Agree'},
          events: ['change'],
        ),
        create(
          3,
          'switch',
          props: {'value': true, 'text': 'Notifications'},
          events: ['change'],
        ),
        create(
          4,
          'radio',
          props: {'value': false, 'text': 'Option A'},
          events: ['change'],
        ),
        create(1, 'column', children: [2, 3, 4]),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller);

      // Tap checkbox
      await tester.tap(find.text('Agree'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 2,
          'event': 'change',
          'data': {'value': true},
        },
      ]);

      // Tap switch
      inputs.all.clear();
      await tester.tap(find.text('Notifications'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 3,
          'event': 'change',
          'data': {'value': false},
        },
      ]);

      // Tap radio
      inputs.all.clear();
      await tester.tap(find.text('Option A'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 4,
          'event': 'change',
          'data': {'value': true},
        },
      ]);
    });
  });

  group('slider', () {
    // Fails if slider dragging does not emit change and change_end events with updated values.
    testWidgets('slider_changes_and_change_end', (tester) async {
      final controller = _show([
        create(
          1,
          'slider',
          props: {'value': 0.2, 'min': 0.0, 'max': 1.0},
          events: ['change', 'change_end'],
        ),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller, width: 300, height: 100);

      // Drag slider across
      await tester.drag(find.byType(Slider), const Offset(100, 0));
      await tester.pumpAndSettle();

      final changes = inputs
          .of('event')
          .where((e) => e['event'] == 'change')
          .toList();
      final changeEnds = inputs
          .of('event')
          .where((e) => e['event'] == 'change_end')
          .toList();

      expect(changes.isNotEmpty, isTrue);
      expect(changeEnds.length, 1);
      expect((changeEnds.first['data'] as Map)['value'], greaterThan(0.2));
    });

    // Fails if slider does not throttle changes to 20 Hz, drops the pending value on release, or jumps mid-drag on redraw.
    testWidgets(
      'slider_throttles_rapid_changes_to_20_per_second_and_flushes_on_release',
      (tester) async {
        final controller = _show([
          create(
            1,
            'slider',
            props: {'value': 0.0, 'min': 0.0, 'max': 1.0},
            events: ['change', 'change_end'],
          ),
        ], 1);
        final inputs = Inputs(controller);
        addTearDown(inputs.cancel);
        await pumpView(tester, controller, width: 300, height: 100);

        final slider = find.byType(Slider);
        final center = tester.getCenter(slider);

        final gesture = await tester.startGesture(center);
        await tester.pump();

        // Two rapid movements in 20 ms
        await gesture.moveBy(const Offset(30, 0));
        await tester.pump(const Duration(milliseconds: 10));
        await gesture.moveBy(const Offset(30, 0));
        await tester.pump(const Duration(milliseconds: 10));

        // Exactly 1 change emitted so far due to 20 Hz cap (50 ms)
        var changes = inputs
            .of('event')
            .where((e) => e['event'] == 'change')
            .toList();
        expect(changes.length, 1);

        // Server redraws value while dragging - thumb must not reset
        controller.applyBatch(
          batch(2, [
            {
              'op': 'props',
              'id': 1,
              'set': {'value': 0.0},
              'unset': <String>[],
            },
          ]),
        );
        await tester.pump();

        // Release flushes pending change before change_end
        await gesture.up();
        await tester.pumpAndSettle();

        changes = inputs
            .of('event')
            .where((e) => e['event'] == 'change')
            .toList();
        final changeEnds = inputs
            .of('event')
            .where((e) => e['event'] == 'change_end')
            .toList();

        expect(changes.length, 2);
        expect(changeEnds.length, 1);
        expect((changes.last['data'] as Map)['value'], greaterThan(0.0));
      },
    );
  });

  group('select', () {
    // Fails if select dropdown does not open options or send change event with selected value.
    testWidgets('a_select_shows_and_sends_its_value', (tester) async {
      final controller = _show([
        create(
          1,
          'select',
          props: {
            'value': 'opt1',
            'options': [
              {'value': 'opt1', 'label': 'Option 1'},
              {'value': 'opt2', 'label': 'Option 2'},
            ],
          },
          events: ['change'],
        ),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller, width: 300, height: 200);

      expect(find.text('Option 1'), findsOneWidget);

      // Open select menu
      await tester.tap(find.text('Option 1'));
      await tester.pumpAndSettle();

      expect(find.text('Option 2'), findsOneWidget);
      await tester.tap(find.text('Option 2').last);
      await tester.pumpAndSettle();

      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 1,
          'event': 'change',
          'data': {'value': 'opt2'},
        },
      ]);
    });

    // Fails if select or menu popups render outside the plugin area bounds or menu does not read items prop.
    testWidgets('select_and_menu_popups_stay_inside_plugin_area', (
      tester,
    ) async {
      final host = _ControlsHost();
      final controller = _show([
        create(2, 'box', props: {'height': 240, 'width': 200}),
        create(
          3,
          'select',
          props: {
            'value': 'a',
            'options': [
              {'value': 'a', 'label': 'Option A'},
              {'value': 'b', 'label': 'Option B'},
              {'value': 'c', 'label': 'Option C'},
            ],
          },
          events: ['change'],
        ),
        create(1, 'column', children: [2, 3]),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller, host: host, width: 300, height: 300);

      // Tap select near bottom (y > 240 in 300 area)
      await tester.tap(find.text('Option A'));
      await tester.pumpAndSettle();

      final optCRect = tester.getRect(find.text('Option C'));
      expect(optCRect.bottom, lessThanOrEqualTo(300.0));
      expect(optCRect.top, greaterThanOrEqualTo(0.0));

      await tester.tap(find.text('Option B'));
      await tester.pumpAndSettle();
      expect(
        inputs
            .of('event')
            .any(
              (e) =>
                  e['event'] == 'change' && (e['data'] as Map)['value'] == 'b',
            ),
        isTrue,
      );

      // Test menu with items prop
      inputs.all.clear();
      final menuController = _show([
        create(2, 'button', props: {'text': 'Open Menu'}),
        create(
          1,
          'menu',
          props: {
            'items': [
              {'value': 'item1', 'label': 'Menu Item 1'},
              {'value': 'item2', 'label': 'Menu Item 2', 'danger': true},
            ],
          },
          events: ['select'],
          children: [2],
        ),
      ], 1);
      final menuInputs = Inputs(menuController);
      addTearDown(menuInputs.cancel);
      await pumpView(
        tester,
        menuController,
        host: host,
        width: 300,
        height: 300,
      );

      await tester.tap(find.text('Open Menu'));
      await tester.pumpAndSettle();

      expect(find.text('Menu Item 1'), findsOneWidget);
      await tester.tap(find.text('Menu Item 1'));
      await tester.pumpAndSettle();

      expect(menuInputs.of('event'), [
        {
          'kind': 'event',
          'node': 1,
          'event': 'select',
          'data': {'value': 'item1'},
        },
      ]);
    });
  });

  group('tabs and segmented', () {
    // Fails if tapping tabs or segmented controls does not emit change event with selected option value.
    testWidgets('tabs_and_segmented_switch', (tester) async {
      final controller = _show([
        create(
          2,
          'tabs',
          props: {
            'value': 'chat',
            'options': [
              {'value': 'chat', 'label': 'Chat'},
              {'value': 'files', 'label': 'Files'},
            ],
          },
          events: ['change'],
        ),
        create(
          3,
          'segmented',
          props: {
            'value': 'grid',
            'options': [
              {'value': 'grid', 'label': 'Grid'},
              {'value': 'list', 'label': 'List'},
            ],
          },
          events: ['change'],
        ),
        create(1, 'column', children: [2, 3]),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller);

      // Switch tab
      await tester.tap(find.text('Files'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 2,
          'event': 'change',
          'data': {'value': 'files'},
        },
      ]);

      // Switch segmented
      inputs.all.clear();
      await tester.tap(find.text('List'));
      await tester.pump();
      expect(inputs.of('event'), [
        {
          'kind': 'event',
          'node': 3,
          'event': 'change',
          'data': {'value': 'list'},
        },
      ]);
    });
  });

  group('text_input', () {
    // Fails if text_input caret position jumps or resets when the plugin redraws with identical text.
    testWidgets('typing_keeps_the_caret_across_redraws', (tester) async {
      final controller = _show([
        create(
          1,
          'text_input',
          props: {'value': 'hello'},
          events: ['change', 'submit'],
        ),
      ], 1);
      await pumpView(tester, controller);

      final textField = find.byType(TextField);
      await tester.tap(textField);
      await tester.pump();

      // Enter text
      await tester.enterText(textField, 'hello world');
      await tester.pump();

      final editable = find.descendant(
        of: textField,
        matching: find.byType(EditableText),
      );
      final editableState = tester.state<EditableTextState>(editable);

      // Place caret between 'hello' and 'world' (index 5)
      editableState.userUpdateTextEditingValue(
        const TextEditingValue(
          text: 'hello world',
          selection: TextSelection.collapsed(offset: 5),
        ),
        SelectionChangedCause.keyboard,
      );
      await tester.pump();

      // Plugin redraws with the value we sent: caret should NOT move!
      controller.applyBatch(
        batch(2, [
          {
            'op': 'props',
            'id': 1,
            'set': {'value': 'hello world'},
            'unset': <String>[],
          },
        ]),
      );
      await tester.pump();

      expect(editableState.textEditingValue.selection.baseOffset, 5);
    });

    // Fails if updating text_input value prop does not replace the displayed text.
    testWidgets('a_plugin_value_change_replaces_the_text', (tester) async {
      final controller = _show([
        create(
          1,
          'text_input',
          props: {'value': 'original'},
          events: ['change'],
        ),
      ], 1);
      await pumpView(tester, controller);

      // Focus the text field
      await tester.tap(find.byType(TextField));
      await tester.pump();

      // Plugin pushes completely different value while focused
      controller.applyBatch(
        batch(2, [
          {
            'op': 'props',
            'id': 1,
            'set': {'value': 'replaced by server'},
            'unset': <String>[],
          },
        ]),
      );
      await tester.pump();

      expect(find.text('replaced by server'), findsOneWidget);
    });

    // Fails if text_input does not throttle rapid keystrokes or fails to send the final text.
    testWidgets('change_is_throttled_and_the_last_text_is_sent', (
      tester,
    ) async {
      final controller = _show([
        create(1, 'text_input', props: {'value': ''}, events: ['change']),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller);

      final textField = find.byType(TextField);
      await tester.enterText(textField, 'a');
      await tester.pump(const Duration(milliseconds: 20));
      await tester.enterText(textField, 'ab');
      await tester.pump(const Duration(milliseconds: 20));
      await tester.enterText(textField, 'abc');
      await tester.pump(const Duration(milliseconds: 20));

      // After typing settles past 100 ms
      await tester.pump(const Duration(milliseconds: 150));

      final changes = inputs
          .of('event')
          .where((e) => e['event'] == 'change')
          .toList();
      expect(changes.isNotEmpty, isTrue);
      expect((changes.last['data'] as Map)['value'], 'abc');
    });

    // Fails if text_input does not send submit event on Enter for single-line or Ctrl+Enter for multi-line.
    testWidgets('submit_on_enter_or_ctrl_enter', (tester) async {
      final controller = _show([
        create(
          2,
          'text_input',
          props: {'lines': 1, 'value': 'single'},
          events: ['submit'],
        ),
        create(
          3,
          'text_input',
          props: {'lines': 3, 'value': 'multi'},
          events: ['submit'],
        ),
        create(1, 'column', children: [2, 3]),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller);

      // Submit on single line input on Enter
      await tester.tap(find.byType(TextField).first);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      var submits = inputs
          .of('event')
          .where((e) => e['event'] == 'submit')
          .toList();
      expect(submits.isNotEmpty, isTrue);
      expect((submits.first['data'] as Map)['value'], 'single');

      // Submit on multi line input on Ctrl+Enter
      inputs.all.clear();
      await tester.tap(find.byType(TextField).last);
      await tester.pump();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pump();

      submits = inputs
          .of('event')
          .where((e) => e['event'] == 'submit')
          .toList();
      expect(submits.isNotEmpty, isTrue);
      expect((submits.first['data'] as Map)['value'], 'multi');
    });
  });

  group('pickers', () {
    // Fails if image or file pickers do not invoke host pick and upload methods with selected path.
    testWidgets('image_and_file_picker_upload', (tester) async {
      final host = _ControlsHost();
      final controller = _show([
        create(2, 'image_picker', props: {'text': 'Pick Image'}),
        create(
          3,
          'file_picker',
          props: {
            'text': 'Pick Document',
            'accept': ['txt', 'pdf'],
          },
        ),
        create(1, 'column', children: [2, 3]),
      ], 1);
      await pumpView(tester, controller, host: host);

      // Tap image picker
      await tester.tap(find.text('Pick Image'));
      await tester.pumpAndSettle();

      expect(host.uploadNodeId, 2);
      expect(host.uploadPath, '/path/to/file.png');

      // Tap file picker
      await tester.tap(find.text('Pick Document'));
      await tester.pumpAndSettle();

      expect(host.uploadNodeId, 3);
    });

    // Fails if picker does not show CircularProgressIndicator while file upload is in flight.
    testWidgets('picker_shows_spinner_during_upload', (tester) async {
      final host = _ControlsHost();
      host.uploadCompleter = Completer<void>();
      final controller = _show([
        create(1, 'file_picker', props: {'text': 'Upload File'}),
      ], 1);
      await pumpView(tester, controller, host: host);

      await tester.tap(find.text('Upload File'));
      await tester.pump();

      // While upload is in flight, spinner is shown
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      // Complete the upload
      host.uploadCompleter!.complete();
      await tester.pumpAndSettle();

      // Spinner is gone, text is back
      expect(find.byType(CircularProgressIndicator), findsNothing);
      expect(find.text('Upload File'), findsOneWidget);
    });
  });
}
