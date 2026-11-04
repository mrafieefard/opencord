// The media viewer's own window (D86): the "dev.opencord/media_window"
// channel to the runners.

import 'dart:ui';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/native_window.dart';

const _channel = MethodChannel('dev.opencord/media_window');

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  late List<MethodCall> calls;
  Object? Function(MethodCall call) answer = (_) => null;

  setUp(() {
    calls = [];
    answer = (_) => null;
    messenger.setMockMethodCallHandler(_channel, (call) async {
      calls.add(call);
      return answer(call);
    });
  });
  tearDown(() => messenger.setMockMethodCallHandler(_channel, null));

  Future<void> fromNative(String method, [Object? arguments]) =>
      messenger.handlePlatformMessage(
        _channel.name,
        _channel.codec.encodeMethodCall(MethodCall(method, arguments)),
        (_) {},
      );

  test('open_sends_the_placement_and_the_engine', () async {
    answer = (call) => 5;
    final window = ChannelMediaWindow();

    final view = await window.open(
      const MediaWindowPlacement(
        mode: MediaWindowMode.normal,
        normal: WindowGeometry(width: 900, height: 700, x: 10, y: 20),
      ),
    );

    expect(view, 5);
    expect(calls.single.method, 'open');
    expect(calls.single.arguments, {
      'mode': 'normal',
      'engineId': PlatformDispatcher.instance.engineId,
      'width': 900,
      'height': 700,
      'x': 10,
      'y': 20,
    });
  });

  test('open_without_a_runner_answers_null', () async {
    messenger.setMockMethodCallHandler(_channel, null);
    expect(
      await ChannelMediaWindow().open(const MediaWindowPlacement()),
      isNull,
    );

    messenger.setMockMethodCallHandler(
      _channel,
      (call) async => throw PlatformException(code: 'no-view'),
    );
    expect(
      await ChannelMediaWindow().open(const MediaWindowPlacement()),
      isNull,
    );
  });

  test('runner_events_reach_the_streams', () async {
    final window = ChannelMediaWindow();
    final closes = <void>[];
    final statuses = <MediaWindowStatus>[];
    final sizes = <WindowGeometry>[];
    window.closeRequests.listen(closes.add);
    window.status.listen(statuses.add);
    window.geometry.listen(sizes.add);

    await fromNative('closeRequested');
    await fromNative('status', {'mode': 'maximized', 'focused': true});
    await fromNative('geometry', {'width': 800, 'height': 600});
    await pumpEventQueue();

    expect(closes, hasLength(1));
    expect(
      statuses.single,
      const MediaWindowStatus(
        mode: MediaWindowMode.normal,
        maximized: true,
        focused: true,
      ),
    );
    expect(sizes.single, const WindowGeometry(width: 800, height: 600));
  });

  test('controls_go_to_the_runner', () async {
    final window = ChannelMediaWindow();

    await window.setMode(MediaWindowMode.fullscreen);
    await window.minimize();
    await window.focus();
    await window.startDrag();
    await window.startResize(ResizeEdge.bottomRight);
    await window.close();

    expect(calls.map((call) => '${call.method}:${call.arguments}'), [
      'setMode:fullscreen',
      'minimize:null',
      'focus:null',
      'startDrag:null',
      'startResize:bottomRight',
      'close:null',
    ]);
  });

  test('placement_round_trips_and_falls_back', () {
    const placement = MediaWindowPlacement(
      mode: MediaWindowMode.normal,
      normal: WindowGeometry(width: 900, height: 700, x: -5, y: 3),
    );
    expect(MediaWindowPlacement.fromJson(placement.toJson()), placement);

    for (final broken in <Object?>[
      'x',
      {'mode': 'nope'},
      null,
    ]) {
      expect(
        MediaWindowPlacement.fromJson(broken),
        const MediaWindowPlacement(),
      );
    }
    expect(
      MediaWindowPlacement.fromJson({'mode': 'normal'}),
      const MediaWindowPlacement(mode: MediaWindowMode.normal),
    );
  });

  test('without_a_window_nothing_opens', () async {
    expect(
      await const NullMediaWindow().open(const MediaWindowPlacement()),
      isNull,
    );
  });
}
