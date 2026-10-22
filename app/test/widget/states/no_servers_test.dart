import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/app_info.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';

Future<MockApp> _withoutServers(
  WidgetTester tester, {
  List<Uri>? opened,
}) async {
  final app = await MockApp.pump(
    tester,
    overrides: [
      if (opened != null)
        linkLauncherProvider.overrideWithValue((uri) async {
          opened.add(uri);
          return true;
        }),
    ],
  );
  app.repository.debugEmptyServerList();
  await tester.pump(const Duration(milliseconds: 300));
  return app;
}

void main() {
  testWidgets('with no servers the window explains the two ways in (§4.13)', (
    tester,
  ) async {
    final app = await _withoutServers(tester);

    expect(find.text('No servers yet'), findsOneWidget);
    expect(
      find.text('Join a server with an invite link or host your own.'),
      findsOneWidget,
    );
    expect(find.byType(MemberPanel), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('Add server opens the dialog', (tester) async {
    final app = await _withoutServers(tester);

    await tester.tap(find.widgetWithText(OcButton, 'Add server'));
    await tester.pump(const Duration(milliseconds: 300));

    expect(find.byType(AddServerDialog), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('Host your own opens the guide to running a server (§16)', (
    tester,
  ) async {
    final opened = <Uri>[];
    final app = await _withoutServers(tester, opened: opened);

    await tester.tap(find.widgetWithText(OcButton, 'Host your own'));
    await tester.pump();

    expect(opened, [Uri.parse(hostingGuideUrl)]);
    await app.dispose(tester);
  });
}
