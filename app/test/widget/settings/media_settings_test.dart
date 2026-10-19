import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/settings/server/storage_dialogs.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

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

Future<void> _openServerSettings(
  WidgetTester tester, [
  String server = 'Opencord Dev',
]) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text(server),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  expect(find.byType(SettingsDialog), findsOneWidget);
}

Future<void> _openUserSettings(WidgetTester tester) async {
  await tester.tap(
    find.byWidgetPredicate(
      (widget) => widget is OcIconButton && widget.tooltip == 'User settings',
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  expect(find.byType(SettingsDialog), findsOneWidget);
}

/// Adds an S3 bucket through the dialog, as an owner would.
Future<void> _connectBucket(WidgetTester tester, String name) async {
  await tester.tap(
    _inSettings(find.widgetWithText(OcButton, 'Connect a bucket')),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.enterText(find.bySemanticsLabel('Storage name'), name);
  await tester.enterText(
    find.bySemanticsLabel('Endpoint'),
    'https://s3.eu-central-1.wasabisys.com',
  );
  await tester.enterText(find.bySemanticsLabel('Region'), 'eu-central-1');
  await tester.enterText(find.bySemanticsLabel('Bucket'), 'my-bucket');
  await tester.enterText(find.bySemanticsLabel('Access key ID'), 'AKIATEST');
  await tester.enterText(
    find.bySemanticsLabel('Secret access key'),
    'SECRETTEST',
  );
  await tester.tap(find.widgetWithText(OcButton, 'Test'));
  await _pumpFor(tester, const Duration(milliseconds: 500));
  await tester.tap(find.widgetWithText(OcButton, 'Save'));
  await _pumpFor(tester, const Duration(milliseconds: 500));
}

VoidCallback? _saveAction(WidgetTester tester) =>
    tester.widget<OcButton>(find.widgetWithText(OcButton, 'Save')).onPressed;

Future<void> _page(WidgetTester tester, String label) async {
  await tester.tap(_inSettings(find.text(label)).first);
  await _pumpFor(tester, const Duration(milliseconds: 400));
}

void main() {
  testWidgets('uploads_page_normalizes_and_saves', (tester) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester);
    await _page(tester, 'Uploads');

    // Click "Block programs"
    await tester.tap(
      _inSettings(find.widgetWithText(OcButton, 'Block programs')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(_inSettings(find.text('exe')), findsOneWidget);
    expect(_inSettings(find.text('sh')), findsOneWidget);

    // Enter a new extension with spaces and uppercase
    final addField = _inSettings(find.bySemanticsLabel('Add extension'));
    await tester.enterText(addField, ' .TAR.GZ ');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(_inSettings(find.text('tar.gz')), findsOneWidget);

    // Enter an invalid extension to verify inline error
    await tester.enterText(addField, 'bad..ext');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(_inSettings(find.byType(InlineError)), findsOneWidget);

    // Clear invalid field
    await tester.enterText(addField, '');
    await tester.pump();

    // Adjust voice maximum slider
    final slider = _inSettings(find.byType(Slider));
    expect(slider, findsOneWidget);

    // Save changes
    await tester.tap(
      _inSettings(find.widgetWithText(OcButton, 'Save changes')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final settings = app.read(serverProvider(_dev)).data!.uploadSettings;
    expect(settings.blockedExtensions, contains('exe'));
    expect(settings.blockedExtensions, contains('tar.gz'));

    await app.dispose(tester);
  });

  testWidgets('storage_page_is_owner_only_and_requires_a_passing_test', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    // On non-owner server (Rust Berlin), Storage is not listed
    final berlinData = app.read(serverProvider(_berlin)).data!;
    final pages = serverSettingsPages(berlinData, _berlin).map((p) => p.id);
    expect(pages, isNot(contains('storage')));

    // On owner server (Opencord Dev), Storage is listed
    await _openServerSettings(tester, 'Opencord Dev');
    await _page(tester, 'Storage');

    expect(_inSettings(find.text('Local disk')), findsOneWidget);
    expect(_inSettings(find.text('Active')), findsOneWidget);

    // Connect a bucket opens S3Dialog
    await tester.tap(
      _inSettings(find.widgetWithText(OcButton, 'Connect a bucket')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.byType(S3Dialog), findsOneWidget);

    // Save button is initially disabled
    final saveButtonFinder = find.widgetWithText(OcButton, 'Save');
    expect(tester.widget<OcButton>(saveButtonFinder).onPressed, isNull);

    // Fill in required fields
    await tester.enterText(find.bySemanticsLabel('Storage name'), 'My Wasabi');
    await tester.enterText(
      find.bySemanticsLabel('Endpoint'),
      'https://s3.eu-central-1.wasabisys.com',
    );
    await tester.enterText(find.bySemanticsLabel('Region'), 'eu-central-1');
    await tester.enterText(find.bySemanticsLabel('Bucket'), 'my-bucket');
    await tester.enterText(find.bySemanticsLabel('Access key ID'), 'AKIATEST');
    await tester.enterText(
      find.bySemanticsLabel('Secret access key'),
      'SECRETTEST',
    );

    // Run test
    await tester.tap(find.widgetWithText(OcButton, 'Test'));
    await _pumpFor(tester, const Duration(milliseconds: 500));

    // Save button is now enabled
    expect(tester.widget<OcButton>(saveButtonFinder).onPressed, isNotNull);
    await tester.tap(saveButtonFinder);
    await _pumpFor(tester, const Duration(milliseconds: 500));

    // Now listed in backends
    expect(_inSettings(find.text('My Wasabi')), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets(
    'editing_a_bucket_renames_without_a_test_and_needs_both_keys_to_change_it',
    (tester) async {
      final app = await MockApp.pump(tester);
      await _openServerSettings(tester, 'Opencord Dev');
      await _page(tester, 'Storage');
      await _connectBucket(tester, 'My Wasabi');

      Future<void> openEdit(String name) async {
        await tester.tap(_inSettings(find.byTooltip('Actions for $name')));
        await _pumpFor(tester, const Duration(milliseconds: 200));
        await tester.tap(find.text('Edit').last);
        await _pumpFor(tester, const Duration(milliseconds: 300));
      }

      // The saved key is only a hint: answers carry it masked.
      await openEdit('My Wasabi');
      expect(find.text('Access key ID (saved: ••••TEST)'), findsOneWidget);

      // A rename needs no test, and sends no bucket settings.
      await tester.enterText(find.bySemanticsLabel('Storage name'), 'Archive');
      await tester.pump();
      expect(_saveAction(tester), isNotNull);
      await tester.tap(find.widgetWithText(OcButton, 'Save'));
      await _pumpFor(tester, const Duration(milliseconds: 500));
      expect(find.byType(S3Dialog), findsNothing);
      expect(_inSettings(find.text('Archive')), findsOneWidget);

      // A new bucket needs both keys, and a passing test of these values.
      await openEdit('Archive');
      await tester.enterText(find.bySemanticsLabel('Bucket'), 'other-bucket');
      await tester.pump();
      expect(_saveAction(tester), isNull);
      expect(
        find.textContaining('needs its access key and secret again'),
        findsOneWidget,
      );
      await tester.enterText(
        find.bySemanticsLabel('Access key ID'),
        'AKIANEW1',
      );
      await tester.enterText(
        find.bySemanticsLabel('Secret access key'),
        'SECRETNEW',
      );
      await tester.pump();
      expect(_saveAction(tester), isNull);
      await tester.tap(find.widgetWithText(OcButton, 'Test'));
      await _pumpFor(tester, const Duration(milliseconds: 500));
      expect(_saveAction(tester), isNotNull);

      // Any change after the test locks Save again.
      await tester.enterText(find.bySemanticsLabel('Region'), 'eu-west-1');
      await tester.pump();
      expect(_saveAction(tester), isNull);

      await app.dispose(tester);
    },
  );

  testWidgets('role_limit_field_validates_against_my_limit', (tester) async {
    final app = await MockApp.pump(tester);
    await _openServerSettings(tester, 'Opencord Dev');
    await _page(tester, 'Roles');

    // Make self non-owner with a 20 MB limit to test "can't set more than your own limit"
    final data = app.read(serverProvider(_dev)).data!;
    final myRole = Role(
      id: 99,
      name: 'Manager',
      color: 0,
      position: 10,
      permissions: Permissions.manageRoles,
      hoist: false,
      mentionable: false,
      maxUploadBytes: 20 * 1024 * 1024,
    );
    final everyone = data.roles[data.info.everyoneRoleId]!.copyWith(
      maxUploadBytes: () => 10 * 1024 * 1024,
    );
    app.read(serverProvider(_dev).notifier).apply(RoleUpserted(_dev, everyone));
    app.read(serverProvider(_dev).notifier).apply(RoleUpserted(_dev, myRole));
    app
        .read(serverProvider(_dev).notifier)
        .apply(
          MemberUpserted(_dev, data.selfMember!.copyWith(roleIds: [myRole.id])),
        );
    app
        .read(serverProvider(_dev).notifier)
        .apply(
          ServerInfoChanged(
            _dev,
            ServerInfo(
              serverIdHex: data.info.serverIdHex,
              name: data.info.name,
              description: data.info.description,
              ownerId: 9999, // someone else is owner
              openJoin: data.info.openJoin,
              everyoneRoleId: data.info.everyoneRoleId,
            ),
          ),
        );
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Select Contributor role (position 1, lower than position 10)
    await tester.tap(_inSettings(find.text('Contributor')).first);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Turn off "Not set" so the limit field becomes active
    await tester.tap(
      _inSettings(
        find.byWidgetPredicate(
          (widget) => widget is OcSwitch && widget.semanticLabel == 'Not set',
        ),
      ),
    );
    await tester.pump();

    final limitField = _inSettings(
      find.byKey(const Key('role-max-upload-size')),
    );
    await tester.ensureVisible(limitField);
    await tester.pump();

    // Try setting 30 MB (exceeding self's 20 MB limit)
    await tester.enterText(limitField, '30');
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(
      _inSettings(
        find.textContaining("You can't set more than your own limit"),
      ),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('cache_row_clears', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openUserSettings(tester);
    await _page(tester, 'Windows & behavior');

    expect(_inSettings(find.text('12.0 MB')), findsOneWidget);
    await tester.tap(_inSettings(find.widgetWithText(OcButton, 'Clear')));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(_inSettings(find.text('0 B')), findsOneWidget);
    await app.dispose(tester);
  });
}
