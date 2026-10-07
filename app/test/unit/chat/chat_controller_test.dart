import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/chat_controller.dart';

class _Composer {
  var focused = 0;

  void focus() => focused++;
}

void main() {
  test('a composer that went is no longer focused', () {
    final controller = ChatController();
    final composer = _Composer();
    // Two tear-offs of one method are equal, not identical.
    controller
      ..attachComposer(composer.focus)
      ..detachComposer(composer.focus)
      ..focusComposer();

    expect(composer.focused, 0);
  });
}
