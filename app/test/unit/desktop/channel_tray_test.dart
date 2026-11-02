import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/desktop/channel_tray.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

const _channel = MethodChannel('dev.opencord/tray');

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  late List<MethodCall> calls;

  setUp(() {
    calls = [];
    messenger.setMockMethodCallHandler(_channel, (call) async {
      calls.add(call);
      return true;
    });
  });
  tearDown(() => messenger.setMockMethodCallHandler(_channel, null));

  Future<void> fromNative(String method, [Object? arguments]) =>
      messenger.handlePlatformMessage(
        _channel.name,
        _channel.codec.encodeMethodCall(MethodCall(method, arguments)),
        (_) {},
      );

  test('showing sends the icon, tooltip and menu to the runner', () async {
    final tray = ChannelTray(platform: TargetPlatform.windows);

    await tray.show(const TrayState(unread: 2, muted: true));

    final show = calls.single;
    expect(show.method, 'show');
    final args = show.arguments as Map;
    expect(args['tooltip'], '2 unread messages');
    expect(args['template'], isFalse);
    final icons = (args['icons'] as List).cast<Map>();
    expect(icons.map((icon) => icon['size']), containsAll([16, 32]));
    for (final icon in icons) {
      final size = icon['size'] as int;
      expect((icon['rgba'] as Uint8List).length, size * size * 4);
    }
    final menu = (args['menu'] as List).cast<Map>();
    final mute = menu.firstWhere((item) => item['label'] == 'Mute');
    expect(mute['id'], 3);
    expect(mute['checked'], isTrue);
    final status = menu.firstWhere((item) => item['label'] == 'Status');
    expect((status['children'] as List), hasLength(SelfPresence.values.length));
    expect(tray.available, isTrue);
  });

  test('the same state is not sent twice', () async {
    final tray = ChannelTray(platform: TargetPlatform.windows);

    await tray.show(const TrayState());
    await tray.show(const TrayState());

    expect(calls, hasLength(1));
  });

  test('on macOS the icon is a template image, whatever the desktop', () async {
    final tray = ChannelTray(platform: TargetPlatform.macOS);

    await tray.show(const TrayState(dark: true));
    await tray.show(const TrayState(dark: false));

    final [dark, light] = [for (final call in calls) call.arguments as Map];
    expect(dark['template'], isTrue);
    expect(
      ((dark['icons'] as List).first as Map)['rgba'],
      ((light['icons'] as List).first as Map)['rgba'],
    );
  });

  test('clicks in the tray become actions', () async {
    final tray = ChannelTray(platform: TargetPlatform.windows);
    await tray.show(const TrayState());
    final actions = <TrayAction>[];
    tray.actions.listen(actions.add);

    await fromNative('activate');
    await fromNative('menu', 3);
    await fromNative('menu', 7);
    await Future<void>.delayed(Duration.zero);

    expect(actions, const [OpenWindow(), ToggleMute(), QuitApp()]);
  });

  test('a runner without a tray leaves it unavailable', () async {
    messenger.setMockMethodCallHandler(_channel, (call) async => false);
    final tray = ChannelTray(platform: TargetPlatform.windows);

    await tray.show(const TrayState());

    expect(tray.available, isFalse);
  });
}
