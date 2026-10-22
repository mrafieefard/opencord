import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// What the app put on the clipboard, for the rest of the test.
class ClipboardSpy {
  ClipboardSpy(WidgetTester tester) {
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') {
        copied = (call.arguments as Map)['text'] as String?;
      }
      if (call.method == 'Clipboard.getData') {
        return copied == null ? null : {'text': copied};
      }
      if (call.method == 'Clipboard.hasStrings') {
        return {'value': copied != null && copied!.isNotEmpty};
      }
      return null;
    });
    addTearDown(
      () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
    );
  }

  String? copied;
}
