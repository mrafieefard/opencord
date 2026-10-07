import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/dialogs/identity_changed_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';
import '../../support/pump.dart';

const _dev = 'opencord.example:7710';
const _fingerprint =
    '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _iconButton(String tooltip) => find.byWidgetPredicate(
  (widget) => widget is OcIconButton && widget.tooltip == tooltip,
);

Finder _button(String label) => find.widgetWithText(OcButton, label);

/// Text fields inside the open dialog (the composer and searches are text
/// fields too).
Finder get _fields => find.descendant(
  of: find.byType(OcDialog),
  matching: find.byType(TextField),
);

Future<void> _openAddServer(WidgetTester tester) async {
  await tester.tap(_iconButton('Add server'));
  await _pumpFor(tester, const Duration(milliseconds: 200));
  expect(find.byType(AddServerDialog), findsOneWidget);
}

Future<void> _serverMenu(WidgetTester tester, String item) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text(item));
  await _pumpFor(tester, const Duration(milliseconds: 600));
}

void main() {
  group('add server', () {
    testWidgets('an invite with a fingerprint joins straight away', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openAddServer(tester);

      await tester.enterText(
        _fields,
        'opencord://new.example:7710/invite/abc123#fp=$_fingerprint',
      );
      await tester.tap(_button('Join'));
      await _pumpFor(tester, const Duration(milliseconds: 1500));

      expect(find.byType(AddServerDialog), findsNothing);
      expect(app.read(currentServerProvider), 'new.example:7710');
      await app.dispose(tester);
    });

    testWidgets('an address without a fingerprint asks to verify it', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openAddServer(tester);

      await tester.enterText(_fields, 'lab.example:7710');
      await tester.tap(_button('Join'));
      await _pumpFor(tester, const Duration(milliseconds: 1500));

      expect(find.text('Verify server'), findsOneWidget);
      expect(
        find.textContaining(RegExp(r'^[0-9A-F]{4} [0-9A-F]{4}')),
        findsOneWidget,
      );
      await tester.tap(_button('Trust and connect'));
      await tester.pump();
      expect(find.text('Connecting to lab.example…'), findsOneWidget);
      await _pumpFor(tester, const Duration(milliseconds: 1500));

      expect(find.byType(AddServerDialog), findsNothing);
      expect(app.read(currentServerProvider), 'lab.example:7710');
      await app.dispose(tester);
    });

    testWidgets('what is wrong shows inline', (tester) async {
      final app = await MockApp.pump(tester);
      await _openAddServer(tester);

      await tester.tap(_button('Join'));
      await tester.pump();
      expect(find.text('Paste an invite link or an address.'), findsOneWidget);

      await tester.enterText(_fields, 'not a link');
      await tester.tap(_button('Join'));
      await _pumpFor(tester, const Duration(milliseconds: 1500));
      expect(
        find.text(
          'Enter an invite link (opencord://…) or a host:port address.',
        ),
        findsOneWidget,
      );
      await app.dispose(tester);
    });

    testWidgets('the owner claim token hides under "I\'m the owner"', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openAddServer(tester);
      expect(_fields, findsOneWidget);

      await tester.tap(find.text("I'm the owner"));
      await tester.pump();
      expect(_fields, findsNWidgets(2));
      await tester.enterText(
        _fields.first,
        'opencord://mine.example:7710/invite/abc#fp=$_fingerprint',
      );
      await tester.enterText(_fields.last, 'claim-123');
      await tester.tap(_button('Join'));
      await _pumpFor(tester, const Duration(milliseconds: 2500));

      expect(
        app.read(serverProvider('mine.example:7710')).data?.isOwner,
        isTrue,
      );
      await app.dispose(tester);
    });
  });

  testWidgets('a changed identity blocks until Disconnect or Forget', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    app.repository.debugFingerprintMismatch(_dev);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(find.byType(IdentityChangedDialog), findsOneWidget);
    expect(find.text('TRUSTED FINGERPRINT'), findsOneWidget);
    expect(find.text('FINGERPRINT SHOWN NOW'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.byType(IdentityChangedDialog), findsOneWidget);

    await tester.tap(_button('Forget server'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.byType(IdentityChangedDialog), findsNothing);
    expect(app.repository.servers.map((s) => s.key), isNot(contains(_dev)));
    await app.dispose(tester);
  });

  testWidgets('the invite dialog makes a link to copy and remakes it', (
    tester,
  ) async {
    String? copied;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied = (call.arguments as Map)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final app = await MockApp.pump(tester);
    await _serverMenu(tester, 'Invite people');

    expect(find.byType(InviteDialog), findsOneWidget);
    await tester.tap(_button('Copy'));
    await tester.pump();
    expect(copied, startsWith('opencord://$_dev/invite/'));
    expect(copied, contains('#fp='));
    final first = copied;

    await tester.tap(find.text('1 day'));
    await tester.pump();
    await tester.tap(_button('Make a new link'));
    await _pumpFor(tester, const Duration(milliseconds: 600));
    await tester.tap(_button('Copy'));
    await tester.pump();
    expect(copied, isNot(first));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  group('create channel', () {
    testWidgets('a text channel name turns kebab case and opens', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _serverMenu(tester, 'Create channel');

      await tester.enterText(_fields, 'Release Notes!');
      await tester.pump();
      expect(find.text('release-notes'), findsOneWidget);
      await tester.tap(_button('Create channel'));
      await _pumpFor(tester, const Duration(milliseconds: 800));

      final channels = app.read(serverProvider(_dev)).data!.channels;
      final created = channels.values.firstWhere(
        (channel) => channel.name == 'release-notes',
      );
      expect(app.read(currentChannelProvider), created.id);
      await app.dispose(tester);
    });

    testWidgets('a category’s + creates the channel inside it', (tester) async {
      final app = await MockApp.pump(tester);
      final category = app
          .read(serverProvider(_dev))
          .data!
          .channels
          .values
          .firstWhere((channel) => channel.isCategory);

      await tester.tap(_iconButton('Create channel in ${category.name}'));
      await _pumpFor(tester, const Duration(milliseconds: 200));
      expect(find.text('Create channel in ${category.name}'), findsOneWidget);
      await tester.tap(find.text('Voice'));
      await tester.enterText(_fields, 'Hangout Room');
      await tester.pump();
      await tester.tap(_button('Create channel'));
      await _pumpFor(tester, const Duration(milliseconds: 800));

      final created = app
          .read(serverProvider(_dev))
          .data!
          .channels
          .values
          .firstWhere((channel) => channel.name == 'Hangout Room');
      expect(created.kind, ChannelKind.voice);
      expect(created.parentId, category.id);
      await app.dispose(tester);
    });

    testWidgets('a category is created by name', (tester) async {
      final app = await MockApp.pump(tester);
      await _serverMenu(tester, 'Create category');

      await tester.enterText(_fields, 'Projects');
      await tester.pump();
      await tester.tap(_button('Create category'));
      await _pumpFor(tester, const Duration(milliseconds: 800));

      expect(
        app
            .read(serverProvider(_dev))
            .data!
            .channels
            .values
            .where(
              (channel) => channel.isCategory && channel.name == 'Projects',
            ),
        hasLength(1),
      );
      await app.dispose(tester);
    });
  });

  test('channel names become kebab case', () {
    expect(kebabName('Release  Notes!'), 'release-notes');
    expect(kebabName('Ünïcode Straße'), 'ünïcode-straße');
    expect(kebabName('a__b--c'), 'a__b-c');
  });

  testWidgets('the confirm dialog answers true only for its action', (
    tester,
  ) async {
    final answers = <bool>[];
    await pumpThemed(
      tester,
      Builder(
        builder: (context) => TextButton(
          onPressed: () async => answers.add(
            await confirmAction(
              context,
              title: 'Delete #general?',
              message: 'This deletes #general for everyone.',
              action: 'Delete channel',
            ),
          ),
          child: const Text('open'),
        ),
      ),
    );

    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.tap(_button('Cancel'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.tap(_button('Delete channel'));
    await tester.pumpAndSettle();

    expect(answers, [false, true]);
  });
}
