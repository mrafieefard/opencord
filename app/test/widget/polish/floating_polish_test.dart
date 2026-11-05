import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/onboarding/onboarding_view.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/navigation_history.dart';
import 'package:opencord/features/shell/no_servers.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/title_crumb.dart';
import 'package:opencord/features/voice/voice_dock.dart';
import 'package:opencord/features/window/app_title_bar.dart';
import 'package:opencord/ui/widgets/notched_float.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

void main() {
  group('Task 21: Everything left (polish)', () {
    testWidgets('large_text_keeps_the_frame', (tester) async {
      final app = await MockApp.pump(
        tester,
        size: const Size(1280, 800),
        settings: (s) => s.copyWith(fontSize: AppSettings.maxFontSize),
      );
      await tester.pumpAndSettle();

      // Headers grow past 52 only as far as needed
      final headRect = tester.getRect(find.byKey(ShellKeys.head));
      expect(headRect.height, greaterThanOrEqualTo(52));

      final hdRect = tester.getRect(find.byKey(ShellKeys.hd));
      expect(hdRect.height, greaterThanOrEqualTo(52));

      // Composer and user panel stay level at the bottom
      final meBottom = tester.getBottomRight(find.byKey(ShellKeys.me)).dy;
      final composerBottom = tester
          .getBottomRight(find.byKey(ShellKeys.composer))
          .dy;
      expect(meBottom, closeTo(composerBottom, 1.0));

      // No overflow exception thrown
      expect(tester.takeException(), isNull);

      await app.dispose(tester);
    });

    testWidgets('every_motion_respects_reduced_motion', (tester) async {
      final app = await MockApp.pump(
        tester,
        size: const Size(1280, 800),
        settings: (s) => s.copyWith(reduceMotion: true),
      );
      await tester.pumpAndSettle();

      final server = app.read(currentServerProvider)!;
      final channels = app.read(serverProvider(server)).data!.channels.values;
      final textChan = channels.firstWhere((c) => c.kind.isTextLike);
      final voiceChan = channels.firstWhere((c) => c.kind == ChannelKind.voice);

      app.read(navigationProvider.notifier).openChannel(server, textChan.id);
      app.read(memberPanelProvider.notifier).close();
      await tester.pumpAndSettle();

      // 1. Open pane
      await tester.tap(find.byTooltip('Open the right pane'));
      await tester.pump();
      expect(tester.hasRunningAnimations, isFalse);

      // 2. Open a menu
      final context = tester.element(find.byType(DesktopShell));
      showOcMenu(
        context: context,
        position: const Offset(200, 200),
        entries: [OcMenuItem(label: 'Item', onSelected: () {})],
      );
      await tester.pump();
      expect(tester.hasRunningAnimations, isFalse);

      // Close menu
      await tester.tap(find.text('Item'));
      await tester.pump();
      expect(tester.hasRunningAnimations, isFalse);

      // 3. Voice card
      await app.container
          .read(voiceSessionProvider.notifier)
          .join(server, voiceChan.id);
      await tester.pump();
      expect(tester.hasRunningAnimations, isFalse);

      // 4. The dock with it on
      app.read(navigationProvider.notifier).openChannel(server, voiceChan.id);
      await tester.pumpAndSettle();
      expect(find.byType(VoiceDock), findsOneWidget);
      expect(tester.hasRunningAnimations, isFalse);

      await app.dispose(tester);
    });

    testWidgets('no_servers_view_has_no_notch', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));

      // Remove all servers and contacts
      for (final s in [...app.repository.servers]) {
        await app.repository.removeServer(s.key);
      }
      app.repository.privateWorld.contacts.clear();
      app.repository.emit(const PrivateStateChanged());
      await tester.pumpAndSettle();

      expect(find.byType(NoServersView), findsOneWidget);
      final notchedFloat = tester.widget<NotchedFloat>(
        find.byKey(ShellKeys.main),
      );
      expect(notchedFloat.notch.value, 0.0);

      await app.dispose(tester);
    });

    testWidgets('onboarding_has_the_row', (tester) async {
      final app = await MockApp.pump(
        tester,
        size: const Size(1280, 800),
        withIdentity: false,
      );
      await tester.pumpAndSettle();

      expect(find.byType(OnboardingView), findsOneWidget);
      expect(find.byType(AppTitleBar), findsOneWidget);
      expect(find.byType(TitleCrumb), findsNothing);
      expect(find.byType(HistoryArrows), findsNothing);

      await app.dispose(tester);
    });
  });
}
