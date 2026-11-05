import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/hotkeys.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/rust/core_mapping.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

void main() {
  test('a_mouse_accelerator_round_trips', () {
    for (final (text, button) in [
      ('Mouse3', MouseButton.middle),
      ('Mouse4', MouseButton.back),
      ('CTRL+Mouse4', MouseButton.back),
      ('CTRL+SHIFT+Mouse5', MouseButton.forward),
    ]) {
      final accelerator = Accelerator.parse(text);

      expect(accelerator?.button, button, reason: text);
      expect(accelerator?.key, isNull, reason: text);
      expect(accelerator.toString(), text);
    }
    expect(
      Accelerator.parse('CTRL+SHIFT+Mouse5'),
      const Accelerator(button: MouseButton.forward, ctrl: true, shift: true),
    );
    expect(
      Accelerator.parse('Mouse4'),
      isNot(const Accelerator(button: MouseButton.forward)),
    );
    expect(
      [for (final button in MouseButton.values) button.mask],
      [kMiddleMouseButton, kBackMouseButton, kForwardMouseButton],
    );
  });

  test('its_label_is_the_specs', () {
    String label(String text, {bool mac = false}) =>
        Accelerator.parse(text)!.label(mac: mac);

    // Linux and Windows.
    expect(label('Mouse3'), 'Middle Mouse');
    expect(label('Mouse4'), 'Mouse 4');
    expect(label('Mouse5'), 'Mouse 5');
    expect(label('CTRL+Mouse4'), 'Ctrl + Mouse 4');
    expect(label('CTRL+SHIFT+Mouse5'), 'Ctrl + Shift + Mouse 5');
    // macOS.
    expect(label('Mouse3', mac: true), 'Middle Mouse');
    expect(label('CTRL+Mouse4', mac: true), '⌃Mouse 4');
    expect(label('LOGO+Mouse5', mac: true), '⌘Mouse 5');
  });

  test('other_buttons_dont_parse', () {
    for (final text in [
      'Mouse1',
      'Mouse2',
      'Mouse6',
      'mouse4',
      'CTRL+Mouse1',
      'HYPER+Mouse4',
    ]) {
      expect(Accelerator.parse(text), isNull, reason: text);
    }
  });

  test('an_old_saved_keybind_with_a_key_still_parses', () {
    final accelerator = Accelerator.parse('CTRL+SHIFT+m');

    expect(accelerator?.key, LogicalKeyboardKey.keyM);
    expect(accelerator?.button, isNull);
    expect(accelerator.toString(), 'CTRL+SHIFT+m');
    expect(accelerator?.label(), 'Ctrl + Shift + M');
    expect(Accelerator.parse('Control_R')?.label(), 'Right Ctrl');
  });

  test("the_core's_answer_says_whether_the_mouse_is_bound", () {
    expect(
      hotkeySupportFrom(
        const core.HotkeySupport.global(method: 'the portal', mouse: false),
      ),
      const HotkeySupport.global('the portal', mouse: false),
    );
    expect(
      hotkeySupportFrom(
        const core.HotkeySupport.global(method: 'X11 input', mouse: true),
      ),
      const HotkeysGlobal('X11 input'),
      reason: 'mouse defaults to true',
    );
    expect(
      const HotkeysGlobal('X11 input'),
      isNot(const HotkeysGlobal('X11 input', mouse: false)),
    );
  });
}
