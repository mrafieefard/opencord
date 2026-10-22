import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/desktop/notifications.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/native_window.dart';

import '../../support/app.dart';
import '../../support/fake_notifications.dart';
import '../../support/fake_window.dart';

const _dev = 'opencord.example:7710';
const _self = 1000, _kai = 1001;

Future<MockApp> _pump(
  WidgetTester tester, {
  required FakeNotifications notifications,
  required FakeNativeWindow window,
}) async {
  final app = await MockApp.pump(
    tester,
    window: window,
    overrides: [notificationServiceProvider.overrideWithValue(notifications)],
  );
  await tester.pump(const Duration(milliseconds: 300));
  return app;
}

Future<void> _focus(
  WidgetTester tester,
  FakeNativeWindow window, {
  required bool focused,
}) async {
  window.statusEvents.add(WindowStatus(focused: focused));
  await tester.pump();
}

int _channel(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

void main() {
  testWidgets('a message while the window is away notifies (§15)', (
    tester,
  ) async {
    final notifications = FakeNotifications();
    final window = FakeNativeWindow();
    final app = await _pump(
      tester,
      notifications: notifications,
      window: window,
    );
    await _focus(tester, window, focused: false);
    final devCore = _channel(app, 'dev-core');

    app.repository.debugPostAs(
      _dev,
      devCore,
      'Pushed the fix, <#$devCore> is green again',
      authorId: _kai,
    );
    await tester.pump();

    final shown = notifications.shown.single;
    expect(shown.title, 'Kai Nakamura (#dev-core, Opencord Dev)');
    expect(shown.body, 'Pushed the fix, #dev-core is green again');
    expect(shown.link, startsWith('opencord://$_dev/c/$devCore/'));
    expect(shown.sound, isTrue);
    await app.dispose(tester);
  });

  testWidgets('nothing while the window is focused', (tester) async {
    final notifications = FakeNotifications();
    final app = await _pump(
      tester,
      notifications: notifications,
      window: FakeNativeWindow(),
    );

    app.repository.debugPostAs(
      _dev,
      _channel(app, 'dev-core'),
      'hello <@$_self>',
      authorId: _kai,
    );
    await tester.pump();

    expect(notifications.shown, isEmpty);
    await app.dispose(tester);
  });

  testWidgets('a mention asks for attention until the window is focused', (
    tester,
  ) async {
    final window = FakeNativeWindow();
    final app = await _pump(
      tester,
      notifications: FakeNotifications(),
      window: window,
    );
    await _focus(tester, window, focused: false);

    app.repository.debugPostAs(
      _dev,
      _channel(app, 'dev-core'),
      '<@$_self> can you look?',
      authorId: _kai,
    );
    await tester.pump();
    expect(window.calls, contains('setUrgent:true'));

    await _focus(tester, window, focused: true);
    expect(window.calls.last, 'setUrgent:false');
    await app.dispose(tester);
  });

  testWidgets('a click opens the window at the message', (tester) async {
    final notifications = FakeNotifications();
    final window = FakeNativeWindow();
    final app = await _pump(
      tester,
      notifications: notifications,
      window: window,
    );
    await _focus(tester, window, focused: false);
    final devCore = _channel(app, 'dev-core');
    app.repository.debugPostAs(_dev, devCore, 'over here', authorId: _kai);
    await tester.pump();

    notifications.click(notifications.shown.single);
    await tester.pump(const Duration(milliseconds: 800));

    expect(window.calls, contains('show'));
    expect(app.read(currentChannelProvider), devCore);
    await app.dispose(tester);
  });
}
