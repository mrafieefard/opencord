import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/settings/server/network/add_domain_dialog.dart';
import 'package:opencord/features/settings/server/network/domain_row.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/features/settings/server/network/network_page.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inSettings(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

Future<void> _openNetwork(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(_inSettings(find.text('Domains & network')).first);
  await _pumpFor(tester, const Duration(milliseconds: 400));
  expect(find.byType(ServerNetworkPage), findsOneWidget);
}

Future<void> _tapVisible(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder);
  await tester.pump();
  await tester.tap(finder);
  await tester.pump();
}

/// The text field of the dialog on top of the settings.
Finder get _dialogField => find
    .descendant(
      of: find.byWidgetPredicate(
        (widget) => widget is ValueDialog || widget is AddressDialog,
      ),
      matching: find.byType(EditableText),
    )
    .first;

/// The settings row titled [title].
Finder _row(String title) => find
    .ancestor(
      of: _inSettings(find.text(title)),
      matching: find.byType(SettingsRow),
    )
    .first;

/// The Change button on the settings row titled [title].
Finder _changeOn(String title) => find.descendant(
  of: _row(title),
  matching: find.widgetWithText(OcButton, 'Change'),
);

void main() {
  domainTests();

  testWidgets('only the owner has Domains & network', (tester) async {
    final app = await MockApp.pump(tester);
    app.read(navigationProvider.notifier).openServer(_berlin);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final owned = app.read(serverProvider(_dev)).data!;
    final berlin = app.read(serverProvider(_berlin)).data!;
    expect(
      serverSettingsPages(owned, _dev).map((page) => page.id),
      contains('network'),
    );
    expect(
      serverSettingsPages(berlin, _berlin).map((page) => page.id),
      isNot(contains('network')),
    );
    await app.dispose(tester);
  });

  testWidgets('the page shows the address and every port, and lets go when '
      'closed', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    expect(_inSettings(find.text('203.0.113.10')), findsOneWidget);
    for (final label in [
      'Main · TCP 7710',
      'Voice and video · UDP 7711',
      'Music · UDP 7712',
      'Certificates · TCP 80',
      'Local listener · TCP 7719',
    ]) {
      expect(_inSettings(find.text(label)), findsOneWidget, reason: label);
    }
    expect(_inSettings(find.text('Not allowed')), findsOneWidget);
    expect(app.container.exists(networkProvider(_dev)), isTrue);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(find.byType(SettingsDialog), findsNothing);
    expect(
      app.container.exists(networkProvider(_dev)),
      isFalse,
      reason: 'no updates once the page is closed',
    );
    await app.dispose(tester);
  });

  testWidgets('an address set by hand is used, and clears again', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _tapVisible(tester, _changeOn('Set by hand'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(_dialogField, '192.0.2.50');
    await tester.tap(find.widgetWithText(OcButton, 'Save'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(_inSettings(find.text('192.0.2.50')), findsOneWidget);
    expect(_inSettings(find.text('IPv4 · Set by hand')), findsOneWidget);

    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Clear')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(_inSettings(find.text('203.0.113.10')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('UPnP shows the router and the forwarded ports', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _tapVisible(
      tester,
      find.descendant(
        of: _row('Open ports on my router (UPnP)'),
        matching: find.byType(OcSwitch),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      _inSettings(
        find.text('Router 192.168.1.1 · public address 203.0.113.10'),
      ),
      findsOneWidget,
    );
    expect(
      _inSettings(find.textContaining('Router: forwarded')),
      findsNWidgets(3),
    );
    await app.dispose(tester);
  });

  testWidgets('a new main port waits for the next start', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _tapVisible(tester, _changeOn('Main port'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(_dialogField, '70000');
    await tester.tap(find.widgetWithText(OcButton, 'Save'));
    await tester.pump();
    expect(find.text('A port is a number from 1 to 65535.'), findsOneWidget);
    await tester.enterText(_dialogField, '443');
    await tester.tap(find.widgetWithText(OcButton, 'Save'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      _inSettings(find.text('Port 443 from the next start')),
      findsOneWidget,
    );
    expect(_inSettings(find.text('Main · TCP 7710')), findsOneWidget);
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('changes from elsewhere show while the page is open', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    // Not awaited: the mock's delay runs on the test's clock.
    unawaited(
      app.repository.updateNetworkSettings(
        _dev,
        const NetworkSettingsChanges(dnsResolvers: ['9.9.9.9']),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(_inSettings(find.text('9.9.9.9')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a server notice shows as a toast', (tester) async {
    final app = await MockApp.pump(tester);

    app.repository.debugNotice(
      _dev,
      const ServerNotice(
        key: 'network.public_address',
        title: 'Public address changed',
        body: 'The server is now reached at 198.51.100.20.',
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(
      find.text(
        'Public address changed. The server is now reached at 198.51.100.20.',
      ),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(seconds: 6));
    await app.dispose(tester);
  });
}

Future<void> _addDomain(WidgetTester tester, String host) async {
  await _tapVisible(
    tester,
    _inSettings(find.widgetWithText(OcButton, 'Add domain')),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.enterText(
    find
        .descendant(
          of: find.byType(AddDomainDialog),
          matching: find.byType(EditableText),
        )
        .first,
    host,
  );
  await tester.tap(find.widgetWithText(OcButton, 'Next'));
  await _pumpFor(tester, const Duration(milliseconds: 600));
}

void domainTests() {
  testWidgets('a domain is added, shows its records, and is checked', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _addDomain(tester, 'Chat.Example.com');

    expect(find.text('Create the DNS records'), findsOneWidget);
    expect(find.text('A  chat.example.com  →  203.0.113.10'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Check now'));
    await _pumpFor(tester, const Duration(milliseconds: 1600));

    expect(find.byType(AddDomainDialog), findsNothing);
    expect(_inSettings(find.text('chat.example.com:7710')), findsOneWidget);
    expect(
      _inSettings(
        find.textContaining('Own domain · Verified · No certificate · Primary'),
      ),
      findsOneWidget,
      reason: 'the first verified domain is the primary',
    );
    final addresses = app.read(serverProvider(_dev)).data!.addresses;
    expect(addresses.primary?.host, 'chat.example.com');
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('a domain that points elsewhere says so and cannot be '
      'turned on', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _addDomain(tester, 'wrong.example.com');
    await tester.tap(find.widgetWithText(OcButton, 'Check now'));
    await _pumpFor(tester, const Duration(milliseconds: 1600));

    expect(
      _inSettings(find.textContaining('It points to 198.51.100.1')),
      findsOneWidget,
    );
    final row = find.ancestor(
      of: _inSettings(find.text('wrong.example.com:7710')),
      matching: find.byType(DomainRow),
    );
    final toggle = tester.widget<OcSwitch>(
      find.descendant(of: row, matching: find.byType(OcSwitch)),
    );
    expect(toggle.onChanged, isNull);
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('invites offer a choice of domain only with more than one', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final repository = app.repository;
    Future<void> settle() => _pumpFor(tester, const Duration(seconds: 2));
    for (final host in ['one.example.com', 'two.example.com']) {
      unawaited(
        repository.addDomain(_dev, hostname: host, kind: DomainKind.own),
      );
      await settle();
    }
    final ids = [
      for (final domain in repository.debugNetwork(_dev).domains) domain.id,
    ];
    for (final id in ids) {
      unawaited(repository.checkDomain(_dev, id));
      await settle();
    }
    var invite = InviteDialogProbe(tester);

    await invite.open(_dev);
    expect(find.text('THROUGH'), findsNothing, reason: 'one active domain');
    expect(invite.link, startsWith('opencord://one.example.com:7710/invite/'));
    await invite.close();

    unawaited(
      repository.updateDomain(_dev, ids[1], const DomainUpdate(active: true)),
    );
    await settle();
    invite = InviteDialogProbe(tester);
    await invite.open(_dev);
    expect(find.text('THROUGH'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'one.example.com'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.text('two.example.com').last);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Make a new link'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(invite.link, startsWith('opencord://two.example.com:7710/invite/'));
    expect(
      find.textContaining(
        'For a browser: https://two.example.com:7710/invite/',
      ),
      findsOneWidget,
    );
    await invite.close();
    await settle();
    await app.dispose(tester);
  });
}

/// The invite dialog, opened from code.
class InviteDialogProbe {
  InviteDialogProbe(this.tester);

  final WidgetTester tester;

  Future<void> open(String serverKey) async {
    final context = tester.element(find.byType(DesktopShell));
    unawaited(showInvitePeople(context, serverKey: serverKey));
    await _pumpFor(tester, const Duration(milliseconds: 600));
  }

  String get link => tester
      .widgetList<Text>(
        find.descendant(
          of: find.byType(InviteDialog),
          matching: find.byType(Text),
        ),
      )
      .map((text) => text.data ?? '')
      .firstWhere((text) => text.startsWith('opencord://'));

  Future<void> close() async {
    await tester.tap(find.widgetWithText(OcButton, 'Done'));
    await _pumpFor(tester, const Duration(milliseconds: 400));
  }
}
