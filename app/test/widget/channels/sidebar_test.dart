import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/features/channels/voice_panel.dart';

import '../../support/app.dart';

RailItemView railItem(WidgetTester tester, String name) =>
    tester.widget<RailItemView>(
      find.byWidgetPredicate(
        (widget) => widget is RailItemView && widget.name == name,
      ),
    );

ChannelRowView row(WidgetTester tester, String name) =>
    tester.widget<ChannelRowView>(
      find.byWidgetPredicate(
        (widget) => widget is ChannelRowView && widget.name == name,
      ),
    );

int channelId(MockApp app, String name) => app
    .read(serverProvider(app.read(currentServerProvider)!))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

void main() {
  group('server rail', () {
    testWidgets(
      'shows names, the selected server inverted, unread and mentions',
      (tester) async {
        final app = await MockApp.pump(
          tester,
          mutedServers: {'homelab.local:7710'},
        );

        final selected = railItem(tester, 'Opencord Dev');
        final berlin = railItem(tester, 'Rust Berlin');
        final homelab = railItem(tester, 'Homelab');

        expect(selected.selected, isTrue);
        expect(berlin.selected, isFalse);
        expect(berlin.unread, isTrue);
        expect(selected.mentions, greaterThan(0));
        expect(homelab.unread, isFalse, reason: 'muted');
        expect(homelab.connection, RailConnection.connecting);
        await app.dispose(tester);
      },
    );

    testWidgets('a right click mutes a server', (tester) async {
      final app = await MockApp.pump(tester);

      await tester.tap(find.text('Rust Berlin'), buttons: kSecondaryButton);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mute notifications'));
      await tester.pumpAndSettle();

      expect(
        app
            .read(notificationPrefsProvider)
            .serverMuted('rust-berlin.example:7710'),
        isTrue,
      );
      expect(railItem(tester, 'Rust Berlin').unread, isFalse);
      await app.dispose(tester);
    });

    testWidgets('dragging reorders servers and the order is kept', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);

      final from = tester.getCenter(find.text('Homelab'));
      final to = tester.getCenter(find.text('Opencord Dev').first);
      final gesture = await tester.startGesture(
        from,
        kind: PointerDeviceKind.mouse,
      );
      await tester.pump(const Duration(milliseconds: 50));
      for (var step = 1; step <= 10; step++) {
        await gesture.moveTo(
          Offset.lerp(from, to - const Offset(0, 30), step / 10)!,
        );
        await tester.pump(const Duration(milliseconds: 50));
      }
      await gesture.up();
      await tester.pumpAndSettle();

      expect(app.read(serverListProvider).first.key, 'homelab.local:7710');
      expect(app.store.read(serverOrderKey), contains('homelab.local:7710'));
      await app.dispose(tester);
    });
  });

  group('channel sidebar', () {
    testWidgets('the header shows who is online', (tester) async {
      final app = await MockApp.pump(tester);

      expect(
        find.textContaining(RegExp(r'^\d+ online · 13 members$')),
        findsOneWidget,
      );
      await app.dispose(tester);
    });

    testWidgets('rows show the last message, its time and the badges', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);

      final general = row(tester, 'general');
      final help = row(tester, 'help');

      expect(
        general.preview,
        'Priya Shah: Thanks @Alex Rivera! Shorter is better.',
      );
      expect(general.time, matches(RegExp(r'^\d\d:\d\d$')));
      expect(general.unread, greaterThan(0));
      expect(general.mentions, greaterThan(0));
      expect(help.unread, 0);
      expect(row(tester, 'welcome').locked, isTrue, reason: 'read-only');
      expect(row(tester, 'maintainers').locked, isTrue, reason: 'private');
      expect(row(tester, 'announcements').kind, ChannelKind.announcement);
      await app.dispose(tester);
    });

    testWidgets('clicking a row opens the channel', (tester) async {
      final app = await MockApp.pump(tester);

      await tester.tap(
        find.byWidgetPredicate(
          (w) => w is ChannelRowView && w.name == 'dev-core',
        ),
      );
      await tester.pump();

      expect(app.read(currentChannelProvider), channelId(app, 'dev-core'));
      expect(row(tester, 'dev-core').selected, isTrue);
      await app.dispose(tester);
    });

    testWidgets('a collapsed category keeps only the open channel', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      app
          .read(navigationProvider.notifier)
          .openChannel(
            app.read(currentServerProvider)!,
            channelId(app, 'general'),
          );
      await tester.pump();

      await tester.tap(find.text('TEXT CHANNELS'));
      await tester.pump();

      expect(
        find.byWidgetPredicate(
          (w) => w is ChannelRowView && w.name == 'general',
        ),
        findsOneWidget,
      );
      expect(
        find.byWidgetPredicate((w) => w is ChannelRowView && w.name == 'help'),
        findsNothing,
      );
      await app.dispose(tester);
    });

    testWidgets(
      'a voice channel lists its people, and joining shows the voice panel',
      (tester) async {
        final app = await MockApp.pump(tester);

        final live = find.byType(LivePill).evaluate().length;
        await tester.tap(find.text('General').first);
        await tester.pump(const Duration(milliseconds: 100));
        final connected = find.text('Voice connected').evaluate().length;
        await tester.tap(find.bySemanticsLabel('Camera'));
        await tester.pump();
        final camera = app.read(voiceSessionProvider).camera;
        await tester.tap(
          find.descendant(
            of: find.byType(VoiceConnectedPanel),
            matching: find.bySemanticsLabel('Disconnect'),
          ),
        );
        await tester.pump();

        expect(live, 1);
        expect(connected, 1);
        expect(camera, isTrue);
        expect(find.text('Voice connected'), findsNothing);
        await app.dispose(tester);
      },
    );

    testWidgets('the user panel mutes and changes presence', (tester) async {
      final app = await MockApp.pump(tester);

      await tester.tap(find.bySemanticsLabel('Mute'));
      await tester.pump();
      final muted = find.bySemanticsLabel('Unmute').evaluate().length;
      await tester.tap(find.bySemanticsLabel(RegExp('Change status')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Do not disturb'));
      await tester.pumpAndSettle();

      expect(muted, 1);
      expect(app.read(selfPresenceProvider), SelfPresence.doNotDisturb);
      expect(
        find.text('Do not disturb'),
        findsOneWidget,
        reason: 'the panel shows it',
      );
      await app.dispose(tester);
    });

    testWidgets('the quick audio menu changes the input device', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);

      await tester.tap(find.bySemanticsLabel('Audio options').first);
      await tester.pumpAndSettle();
      await tester.tap(find.text('USB headset microphone'));
      await tester.pump();

      expect(find.text('INPUT DEVICE'), findsOneWidget);
      expect(find.text('OUTPUT VOLUME'), findsOneWidget);
      expect(
        app.read(audioSettingsProvider).inputDevice,
        'USB headset microphone',
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      await app.dispose(tester);
    });
  });
}
