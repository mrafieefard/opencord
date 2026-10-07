import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';

const _homelab = 'homelab.local:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 100);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _countdown(String seconds) =>
    find.textContaining(RegExp('Retrying in [$seconds] s'));

/// What live regions say: screen readers announce each change.
List<String> _liveLabels(WidgetTester tester) {
  final labels = <String>[];
  void visit(SemanticsNode node) {
    final data = node.getSemanticsData();
    if (data.flagsCollection.isLiveRegion) labels.add(data.label);
    node.visitChildren((child) {
      visit(child);
      return true;
    });
  }

  for (final view in tester.binding.renderViews) {
    visit(view.owner!.semanticsOwner!.rootSemanticsNode!);
  }
  return labels;
}

Future<MockApp> _openHomelab(WidgetTester tester) async {
  final app = await MockApp.pump(tester);
  app.read(navigationProvider.notifier).openServer(_homelab);
  await _pumpFor(tester, const Duration(milliseconds: 600));
  return app;
}

void main() {
  testWidgets('an unreachable server shows a banner counting down (§4.13)', (
    tester,
  ) async {
    final app = await _openHomelab(tester);
    expect(
      app.read(serverProvider(_homelab)).connection.phase,
      ConnectionPhase.reconnecting,
    );
    expect(_countdown('5-8'), findsOneWidget);

    await _pumpFor(tester, const Duration(seconds: 3));

    expect(_countdown('2-5'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('screen readers hear the change, not every second of it', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final app = await _openHomelab(tester);
    final said = _liveLabels(tester);

    await _pumpFor(tester, const Duration(seconds: 3));

    expect(said, isNotEmpty);
    expect(_liveLabels(tester), said);
    await app.dispose(tester);
    semantics.dispose();
  });

  testWidgets('Retry now tries again at once', (tester) async {
    final app = await _openHomelab(tester);

    await tester.tap(find.widgetWithText(OcButton, 'Retry now'));
    await tester.pump();

    expect(
      app.read(serverProvider(_homelab)).connection.phase,
      ConnectionPhase.connecting,
    );
    await _pumpFor(tester, const Duration(seconds: 1));
    expect(_countdown('7-8'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('the cached chat is greyed out meanwhile', (tester) async {
    final app = await _openHomelab(tester);

    final opacity = tester.widget<Opacity>(
      find
          .ancestor(
            of: find.byType(MessageList),
            matching: find.byType(Opacity),
          )
          .first,
    );

    expect(opacity.opacity, lessThan(1));
    await app.dispose(tester);
  });

  testWidgets('a connected server has no banner', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.textContaining("Can't reach server"), findsNothing);
    await app.dispose(tester);
  });
}
