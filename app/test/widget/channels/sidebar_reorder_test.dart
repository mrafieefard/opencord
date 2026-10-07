import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _row(String text) => find
    .descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text(text),
    )
    .first;

Future<void> _drag(WidgetTester tester, Finder from, Offset to) async {
  final start = tester.getCenter(from);
  final gesture = await tester.startGesture(start);
  await tester.pump(const Duration(milliseconds: 100));
  // Reorderable lists follow the pointer in small steps.
  for (var step = 1; step <= 10; step++) {
    await gesture.moveTo(Offset.lerp(start, to, step / 10)!);
    await tester.pump(const Duration(milliseconds: 30));
  }
  await gesture.up();
  await _pumpFor(tester, const Duration(milliseconds: 800));
}

Channel _named(MockApp app, String server, String name) => app
    .read(serverProvider(server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name);

void main() {
  testWidgets('channels drag into a new order in the sidebar (§16)', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    await _drag(
      tester,
      _row('off-topic'),
      tester.getCenter(_row('general')) - const Offset(0, 20),
    );

    expect(
      _named(app, _dev, 'off-topic').position,
      lessThan(_named(app, _dev, 'general').position),
    );
    await app.dispose(tester);
  });

  testWidgets('categories drag as blocks', (tester) async {
    final app = await MockApp.pump(tester);

    await _drag(
      tester,
      _row('VOICE'),
      tester.getCenter(_row('INFORMATION')) - const Offset(0, 16),
    );

    expect(
      _named(app, _dev, 'Voice').position,
      lessThan(_named(app, _dev, 'Information').position),
    );
    await app.dispose(tester);
  });

  testWidgets('without Manage channels nothing moves', (tester) async {
    final app = await MockApp.pump(tester);
    app.read(navigationProvider.notifier).openServer(_berlin);
    await _pumpFor(tester, const Duration(milliseconds: 500));
    final channels =
        app
            .read(serverProvider(_berlin))
            .data!
            .channels
            .values
            .where((channel) => !channel.isCategory && channel.kind.isTextLike)
            .toList()
          ..sort((a, b) => a.position.compareTo(b.position));
    final before = {
      for (final channel in channels) channel.id: channel.position,
    };

    await _drag(
      tester,
      _row(channels.last.name),
      tester.getCenter(_row(channels.first.name)) - const Offset(0, 20),
    );

    expect({
      for (final channel in channels)
        channel.id: _named(app, _berlin, channel.name).position,
    }, before);
    await app.dispose(tester);
  });

  testWidgets('a click on a channel still opens it', (tester) async {
    final app = await MockApp.pump(tester);

    await tester.tap(_row('help'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.read(currentChannelProvider), _named(app, _dev, 'help').id);
    await app.dispose(tester);
  });
}
