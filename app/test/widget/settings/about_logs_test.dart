import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/logs/session_log.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

const _current = '/data/logs/opencord-2026-10-30T14-32-05.log';
const _crashed = '/data/logs/opencord-2026-10-30T09-05-00.log';

class _FakeActions implements SessionLogActions {
  _FakeActions({this.location = '/home/me/opencord-log.txt', this.fail});

  final String? location;
  final Exception? fail;
  final suggested = <String>[];
  final exported = <(String, String)>[];
  final opened = <String>[];

  @override
  Future<String?> saveLocation(String suggestedName) async {
    suggested.add(suggestedName);
    return location;
  }

  @override
  Future<void> export(String source, String destination) async {
    if (fail case final fail?) throw fail;
    exported.add((source, destination));
  }

  @override
  Future<void> openFolder(String dir) async => opened.add(dir);
}

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<MockApp> _about(
  WidgetTester tester,
  _FakeActions actions, {
  SessionLog log = const SessionLog(current: _current, dir: '/data/logs'),
}) async {
  final app = await MockApp.pump(
    tester,
    platform: TargetPlatform.linux,
    overrides: [
      sessionLogProvider.overrideWithValue(log),
      sessionLogActionsProvider.overrideWithValue(actions),
    ],
  );
  await tester.tap(
    find.byWidgetPredicate(
      (widget) => widget is OcIconButton && widget.tooltip == 'User settings',
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(
    find.descendant(
      of: find.byType(SettingsDialog),
      matching: find.text('About'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  return app;
}

Finder _button(String row) => find.descendant(
  of: find.ancestor(of: find.text(row), matching: find.byType(Row)).first,
  matching: find.byType(OcButton),
);

Future<void> _press(WidgetTester tester, String row) async {
  await tester.ensureVisible(find.text(row));
  await tester.tap(_button(row));
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

void main() {
  testWidgets('shows export and open rows', (tester) async {
    final app = await _about(tester, _FakeActions());
    expect(find.text('LOGS'), findsOneWidget);
    expect(find.text("Export this session's log"), findsOneWidget);
    expect(find.text('Open logs folder'), findsOneWidget);
    expect(find.text("Export the last session's log"), findsNothing);
    expect(
      find.text(
        'Logs never contain passwords, keys, messages or addresses. '
        'The last 10 sessions are kept.',
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('last session row only when it crashed', (tester) async {
    final actions = _FakeActions();
    final app = await _about(
      tester,
      actions,
      log: const SessionLog(
        current: _current,
        previousCrashed: _crashed,
        dir: '/data/logs',
      ),
    );
    expect(find.text("Export the last session's log"), findsOneWidget);
    expect(find.text('It ended unexpectedly'), findsOneWidget);
    await _press(tester, "Export the last session's log");
    expect(actions.suggested, ['opencord-log-2026-10-30-0905.txt']);
    expect(actions.exported, [(_crashed, '/home/me/opencord-log.txt')]);
    await app.dispose(tester);
  });

  testWidgets('export uses the suggested name and confirms', (tester) async {
    final actions = _FakeActions();
    final app = await _about(tester, actions);
    await _press(tester, "Export this session's log");
    expect(actions.suggested, ['opencord-log-2026-10-30-1432.txt']);
    expect(actions.exported, [(_current, '/home/me/opencord-log.txt')]);
    expect(find.text('Log saved'), findsOneWidget);
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('a cancelled save exports nothing', (tester) async {
    final actions = _FakeActions(location: null);
    final app = await _about(tester, actions);
    await _press(tester, "Export this session's log");
    expect(actions.exported, isEmpty);
    expect(find.text('Log saved'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a failed export says why', (tester) async {
    final actions = _FakeActions(fail: Exception('disk full'));
    final app = await _about(tester, actions);
    await _press(tester, "Export this session's log");
    expect(find.textContaining('disk full'), findsOneWidget);
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('open folder calls the actions', (tester) async {
    final actions = _FakeActions();
    final app = await _about(tester, actions);
    await _press(tester, 'Open logs folder');
    expect(actions.opened, ['/data/logs']);
    await app.dispose(tester);
  });

  testWidgets('no Logs section without a session log', (tester) async {
    final app = await _about(tester, _FakeActions(), log: const SessionLog());
    expect(find.text("Export this session's log"), findsNothing);
    await app.dispose(tester);
  });
}
