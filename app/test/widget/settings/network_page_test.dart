import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/settings/server/network/add_domain_dialog.dart';
import 'package:opencord/features/settings/server/network/certificate_dialogs.dart';
import 'package:opencord/features/settings/server/network/domain_row.dart';
import 'package:opencord/features/settings/server/network/free_domain_dialog.dart';
import 'package:opencord/features/settings/server/network/tunnel_dialog.dart';
import 'package:opencord/features/settings/server/network/tunnel_section.dart';
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
  certificateTests();
  freeDomainAndTunnelTests();

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

/// A verified domain, added and checked through the repository.
Future<void> _verified(MockApp app, WidgetTester tester, String host) async {
  final repository = app.repository;
  unawaited(repository.addDomain(_dev, hostname: host, kind: DomainKind.own));
  await _pumpFor(tester, const Duration(milliseconds: 400));
  final id = app
      .read(networkProvider(_dev))
      .value!
      .domains
      .firstWhere((domain) => domain.hostname == host)
      .id;
  unawaited(repository.checkDomain(_dev, id));
  await _pumpFor(tester, const Duration(milliseconds: 1600));
}

/// Opens the "⋯" menu of [host]'s row.
Future<void> _domainMenu(WidgetTester tester, String host) async {
  await _tapVisible(tester, find.byTooltip('More for $host'));
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

void certificateTests() {
  testWidgets('the first certificate asks for the agreement, then shows each '
      'step and the expiry', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    await _verified(app, tester, 'chat.example.com');
    expect(
      _inSettings(find.textContaining('Verified · No certificate')),
      findsOneWidget,
    );

    await _domainMenu(tester, 'chat.example.com');
    await tester.tap(find.text('Get certificate'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(find.byType(GetCertificateDialog), findsOneWidget);
    final get = find.widgetWithText(OcButton, 'Get certificate');
    expect(tester.widget<OcButton>(get).onPressed, isNull, reason: 'agree');
    await tester.enterText(
      find
          .descendant(
            of: find.byType(GetCertificateDialog),
            matching: find.byType(EditableText),
          )
          .first,
      'owner@example.com',
    );
    await tester.tap(
      find.descendant(
        of: find.byType(GetCertificateDialog),
        matching: find.byType(OcSwitch),
      ),
    );
    await tester.pump();
    await tester.tap(get);
    await _pumpFor(tester, const Duration(milliseconds: 450));

    expect(find.byType(GetCertificateDialog), findsNothing);
    expect(
      _inSettings(find.textContaining('Certificate: Creating the account')),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(milliseconds: 1050));
    expect(
      _inSettings(find.textContaining('Certificate: Setting up the challenge')),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(seconds: 4));
    expect(
      _inSettings(find.textContaining('· renews automatically')),
      findsOneWidget,
    );
    final network = app.read(networkProvider(_dev)).value!;
    expect(network.acme.exists, isTrue);
    expect(network.acme.email, 'owner@example.com');
    expect(network.domains.single.trustedCertificate, isTrue);
    await app.dispose(tester);
  });

  testWidgets('a failed request explains why, and renewing asks nothing '
      'once the account exists', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    await _verified(app, tester, 'nocert.example.com');
    final id = app.read(networkProvider(_dev)).value!.domains.single.id;
    unawaited(app.repository.requestCertificate(_dev, id, agreeTerms: true));
    await _pumpFor(tester, const Duration(seconds: 4));

    expect(
      _inSettings(find.textContaining("Port 80 isn't reachable")),
      findsOneWidget,
    );
    expect(_inSettings(find.textContaining('No certificate')), findsOneWidget);
    await _domainMenu(tester, 'nocert.example.com');
    await tester.tap(find.text('Get certificate'));
    await _pumpFor(tester, const Duration(milliseconds: 450));

    expect(find.byType(GetCertificateDialog), findsNothing);
    expect(
      _inSettings(find.textContaining('Certificate: Creating the account')),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(seconds: 4));
    await app.dispose(tester);
  });

  testWidgets('test certificates are labelled and not trusted', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    unawaited(
      app.repository.updateNetworkSettings(
        _dev,
        const NetworkSettingsChanges(acmeStaging: true),
      ),
    );
    await _verified(app, tester, 'chat.example.com');
    final id = app.read(networkProvider(_dev)).value!.domains.single.id;
    unawaited(app.repository.requestCertificate(_dev, id, agreeTerms: true));
    await _pumpFor(tester, const Duration(seconds: 4));

    expect(
      _inSettings(find.textContaining('Test certificate, valid until')),
      findsOneWidget,
    );
    expect(
      app.read(networkProvider(_dev)).value!.domains.single.trustedCertificate,
      isFalse,
    );
    await app.dispose(tester);
  });

  testWidgets('an uploaded certificate needs both parts', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    await _verified(app, tester, 'chat.example.com');

    await _domainMenu(tester, 'chat.example.com');
    await tester.tap(find.text('Upload certificate'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    final fields = find.descendant(
      of: find.byType(UploadCertificateDialog),
      matching: find.byType(EditableText),
    );
    final upload = find.widgetWithText(OcButton, 'Upload');
    await tester.enterText(
      fields.at(0),
      '-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----',
    );
    await tester.pump();
    expect(tester.widget<OcButton>(upload).onPressed, isNull);
    await tester.enterText(
      fields.at(1),
      '-----BEGIN PRIVATE KEY-----\nMIIE\n-----END PRIVATE KEY-----',
    );
    await tester.pump();
    await tester.tap(upload);
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(find.byType(UploadCertificateDialog), findsNothing);
    expect(
      _inSettings(find.textContaining('Uploaded certificate, valid until')),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(seconds: 4));
    await app.dispose(tester);
  });

  testWidgets('a DNS provider is connected, shown masked, and disconnected', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    final row = _row('Cloudflare');

    await _tapVisible(
      tester,
      find.descendant(
        of: row,
        matching: find.widgetWithText(OcButton, 'Connect'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(_dialogField, 'cf-secret-token-wxyz');
    await tester.tap(find.widgetWithText(OcButton, 'Connect').last);
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(
      _inSettings(find.text('Connected · token ••••wxyz')),
      findsOneWidget,
    );
    expect(find.textContaining('cf-secret-token'), findsNothing);
    await _tapVisible(
      tester,
      find.descendant(
        of: _row('Cloudflare'),
        matching: find.widgetWithText(OcButton, 'Disconnect'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Disconnect').last);
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(app.read(networkProvider(_dev)).value!.dnsProviders, isEmpty);
    await _pumpFor(tester, const Duration(seconds: 4));
    await app.dispose(tester);
  });
}

/// The text fields of the dialog of type [T].
Finder _fieldsOf<T>() =>
    find.descendant(of: find.byType(T), matching: find.byType(EditableText));

void freeDomainAndTunnelTests() {
  testWidgets('a free domain is pointed here, verified and certified by '
      'itself', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Get a free domain')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.byType(FreeDomainDialog), findsOneWidget);
    final get = find.widgetWithText(OcButton, 'Get domain');
    await tester.enterText(
      _fieldsOf<FreeDomainDialog>().at(0),
      'duck-token-0123456789abcdef',
    );
    await tester.enterText(_fieldsOf<FreeDomainDialog>().at(1), 'MyChat');
    await tester.pump();
    expect(tester.widget<OcButton>(get).onPressed, isNull, reason: 'agree');
    await tester.tap(
      find.descendant(
        of: find.byType(FreeDomainDialog),
        matching: find.byType(OcSwitch),
      ),
    );
    await tester.pump();
    await tester.tap(get);
    await _pumpFor(tester, const Duration(seconds: 2));

    expect(find.byType(FreeDomainDialog), findsNothing);
    expect(
      _inSettings(find.textContaining('mychat.duckdns.org')),
      findsWidgets,
    );
    await _pumpFor(tester, const Duration(seconds: 5));
    final network = app.read(networkProvider(_dev)).value!;
    final domain = network.domains.single;
    expect(domain.kind, DomainKind.freeSubdomain);
    expect(domain.status, DomainStatus.verified);
    expect(domain.certificate.state, CertificateState.valid);
    expect(network.provider(DnsProviderKind.duckdns)?.masked, '••••cdef');
    await app.dispose(tester);
  });

  testWidgets('a quick tunnel shows its address, limits and UDP note, and '
      'stops', (tester) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);

    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Use a tunnel')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.byType(TunnelDialog), findsOneWidget);
    expect(find.textContaining('For testing only'), findsOneWidget);
    expect(find.textContaining('carry no UDP'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Start tunnel'));
    await _pumpFor(tester, const Duration(seconds: 3));

    expect(find.byType(TunnelDialog), findsNothing);
    final tunnel = app.read(networkProvider(_dev)).value!.tunnel;
    expect(tunnel.state, TunnelState.running);
    expect(tunnel.hostname, endsWith('.trycloudflare.com'));
    expect(find.byType(TunnelSection), findsOneWidget);
    expect(
      _inSettings(find.textContaining('Quick tunnel · Running')),
      findsOneWidget,
    );
    expect(
      _inSettings(find.textContaining('still need UDP to reach')),
      findsOneWidget,
      reason: 'the tunnel domain says what it cannot carry',
    );

    await _tapVisible(
      tester,
      find.descendant(
        of: find.byType(TunnelSection),
        matching: find.widgetWithText(OcButton, 'Stop'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Stop tunnel'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.byType(TunnelSection), findsNothing);
    expect(app.read(networkProvider(_dev)).value!.domains, isEmpty);
    await app.dispose(tester);
  });

  testWidgets('a named tunnel needs a real token and a hostname', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Use a tunnel')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.tap(find.text('My Cloudflare tunnel'));
    await tester.pump();
    final start = find.widgetWithText(OcButton, 'Start tunnel');
    expect(tester.widget<OcButton>(start).onPressed, isNull);
    expect(find.textContaining('http://localhost:7719'), findsOneWidget);
    await tester.enterText(_fieldsOf<TunnelDialog>().at(0), 'short');
    await tester.enterText(_fieldsOf<TunnelDialog>().at(1), 'chat.example.com');
    await tester.pump();
    await tester.tap(start);
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(find.textContaining('not a tunnel token'), findsOneWidget);
    await tester.enterText(
      _fieldsOf<TunnelDialog>().at(0),
      'eyJhIjoiZmFrZS10dW5uZWwtdG9rZW4tZm9yLXRlc3RzIn0',
    );
    await tester.pump();
    await tester.tap(start);
    await _pumpFor(tester, const Duration(seconds: 3));

    final network = app.read(networkProvider(_dev)).value!;
    expect(network.tunnel.kind, TunnelKind.named);
    expect(network.tunnel.state, TunnelState.running);
    expect(network.domains.single.hostname, 'chat.example.com');
    expect(network.domains.single.kind, DomainKind.tunnel);
    await app.dispose(tester);
  });

  testWidgets('a reverse proxy gets Caddy and Nginx configuration', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Add domain')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(
      _fieldsOf<AddDomainDialog>().first,
      'proxy.example.com',
    );
    await tester.tap(find.text('Through a reverse proxy'));
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Next'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.textContaining('reverse_proxy 127.0.0.1:7719'), findsOneWidget);
    await tester.tap(find.text('Nginx'));
    await tester.pump();
    expect(
      find.textContaining(r'proxy_set_header Upgrade $http_upgrade;'),
      findsOneWidget,
    );
    expect(find.textContaining('proxy_read_timeout 1h;'), findsOneWidget);
    expect(find.textContaining('carry no UDP'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Later'));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets("a domain behind Cloudflare's proxy shows its proxied records, "
      'the port and certificate steps, and takes an origin certificate', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openNetwork(tester);
    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Add domain')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.enterText(
      _fieldsOf<AddDomainDialog>().first,
      'proxied.example.com',
    );
    await tester.tap(find.text("Through Cloudflare's proxy"));
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Next'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.text('Point Cloudflare here'), findsOneWidget);
    expect(find.textContaining('orange cloud'), findsOneWidget);
    expect(find.textContaining('Origin Rule'), findsOneWidget);
    expect(find.textContaining('Full (strict)'), findsOneWidget);
    expect(find.textContaining('carry no UDP'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Later'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    await _tapVisible(
      tester,
      _inSettings(find.byTooltip('More for proxied.example.com')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.text('Upload origin certificate'), findsOneWidget);
    expect(find.text('Get certificate'), findsNothing);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(seconds: 2));
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
