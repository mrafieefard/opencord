import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';
const _kai = 1001;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inSettings(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

Future<void> _openServerSettings(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  expect(find.byType(SettingsDialog), findsOneWidget);
}

Future<void> _page(WidgetTester tester, String label) async {
  await tester.tap(_inSettings(find.text(label)).first);
  await _pumpFor(tester, const Duration(milliseconds: 400));
}

void main() {
  testWidgets('the server menu opens its settings on the overview', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);

    expect(_inSettings(find.text('Overview')), findsWidgets);
    for (final page in ['Members', 'Invites', 'Bans']) {
      expect(_inSettings(find.text(page)), findsOneWidget);
    }
    expect(_inSettings(find.text(_dev)), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('the overview saves name and who may join', (tester) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);

    await tester.enterText(
      _inSettings(find.byType(TextField)).first,
      'Opencord HQ',
    );
    await tester.tap(_inSettings(find.text('Open to anyone with the address')));
    await tester.pump();
    await tester.tap(
      _inSettings(find.widgetWithText(OcButton, 'Save changes')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final info = app.read(serverProvider(_dev)).data!.info;
    expect(info.name, 'Opencord HQ');
    expect(info.openJoin, isTrue);
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('members filter by name and can be kicked', (tester) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);
    await _page(tester, 'Members');

    await tester.enterText(_inSettings(find.byType(TextField)), 'nakam');
    await tester.pump();
    expect(_inSettings(find.text('Kai Nakamura')), findsOneWidget);
    expect(_inSettings(find.text('Mira Okafor')), findsNothing);

    await tester.tap(
      find.byWidgetPredicate(
        (widget) =>
            widget is OcIconButton &&
            widget.tooltip == 'Actions for Kai Nakamura',
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Kick'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Kick'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(app.read(serverProvider(_dev)).data!.members[_kai], isNull);
    await app.dispose(tester);
  });

  testWidgets('screen readers reach each invite\'s Copy and Revoke (§9)', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);
    await _page(tester, 'Invites');
    await tester.tap(
      _inSettings(find.widgetWithText(OcButton, 'Create invite')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));
    await tester.tap(find.widgetWithText(OcButton, 'Done'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.bySemanticsLabel(RegExp('^Revoke ')), findsWidgets);
    expect(find.bySemanticsLabel('Copy link'), findsWidgets);
    await app.dispose(tester);
    semantics.dispose();
  });

  testWidgets('invites are made, listed and revoked', (tester) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);
    await _page(tester, 'Invites');
    final before = _inSettings(find.byTooltip('Copy link')).evaluate().length;

    await tester.tap(
      _inSettings(find.widgetWithText(OcButton, 'Create invite')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));
    await tester.tap(find.widgetWithText(OcButton, 'Done'));
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(_inSettings(find.byTooltip('Copy link')), findsNWidgets(before + 1));

    await tester.tap(
      _inSettings(
        find.byWidgetPredicate(
          (widget) =>
              widget is OcIconButton && widget.tooltip.startsWith('Revoke '),
        ),
      ).first,
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Revoke invite'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(_inSettings(find.byTooltip('Copy link')), findsNWidgets(before));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('a ban is listed and lifted', (tester) async {
    final app = await MockApp.pump(tester);
    app.repository.banMember(_dev, _kai, reason: 'Spam links');
    await _pumpFor(tester, const Duration(milliseconds: 600));
    await _openServerSettings(tester);
    await _page(tester, 'Bans');

    expect(_inSettings(find.text('Kai Nakamura')), findsOneWidget);
    expect(_inSettings(find.textContaining('Spam links')), findsOneWidget);
    await tester.tap(_inSettings(find.widgetWithText(OcButton, 'Unban')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Unban').last);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(_inSettings(find.text('No one is banned.')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('pages need their permission', (tester) async {
    final app = await MockApp.pump(tester);
    app.read(navigationProvider.notifier).openServer(_berlin);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final data = app.read(serverProvider(_berlin)).data!;
    final pages = serverSettingsPages(data, _berlin).map((page) => page.id);
    expect(pages, isNot(contains('overview')));
    expect(pages, isNot(contains('bans')));
    // Listing invites needs Manage server; making one is Invite people.
    expect(pages, isNot(contains('invites')));
    expect(pages, isNot(contains('voice')));
    expect(canOpenServerSettings(data), isFalse);
    await app.dispose(tester);
  });

  testWidgets('Voice & video saves the screen share cap and the AFK channel', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);
    await _page(tester, 'Voice & video');

    Future<void> tapVisible(Finder finder) async {
      await tester.ensureVisible(finder);
      await tester.pump();
      await tester.tap(finder);
      await tester.pump();
    }

    await tapVisible(_inSettings(find.text('1080p')));
    await tapVisible(_inSettings(find.text('60 fps')));
    await tapVisible(_inSettings(find.text('AFK channel')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.text('Pairing').last);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tapVisible(
      _inSettings(find.widgetWithText(OcButton, 'Save changes')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final data = app.read(serverProvider(_dev)).data!;
    final settings = data.voiceSettings;
    expect(settings.screenShareMaxResolution, ScreenShareResolution.p1080);
    expect(settings.screenShareMaxFps, 60);
    expect(data.channels[settings.afkChannelId]?.name, 'Pairing');
    expect(settings.maxVoiceBitrate, 96000, reason: 'untouched');
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });
}
