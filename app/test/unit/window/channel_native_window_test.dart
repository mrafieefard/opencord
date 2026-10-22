import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/window/native_window.dart';

const _channel = MethodChannel('test/window');

/// A call from the platform runner to the app.
Future<void> _fromRunner(String method, Object? arguments) =>
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .handlePlatformMessage(
          _channel.name,
          const StandardMethodCodec().encodeMethodCall(
            MethodCall(method, arguments),
          ),
          (_) {},
        );

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('links handed over before anyone listens are kept (§15)', () async {
    final window = ChannelNativeWindow(_channel);
    await _fromRunner('openLink', 'opencord://a.example:1/c/2');

    final links = <String>[];
    window.links.listen(links.add);
    await Future<void>.delayed(Duration.zero);
    expect(links, ['opencord://a.example:1/c/2']);

    await _fromRunner('openLink', 'opencord://a.example:1/c/3');
    expect(links, ['opencord://a.example:1/c/2', 'opencord://a.example:1/c/3']);
  });
}
