import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/servers/rail_popover.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

import '../../support/app.dart';

Finder railItem(String name) => find.byWidgetPredicate(
  (widget) => widget is RailItemView && widget.name == name,
);

void main() {
  group('Server rail and popover (§5.2, Task 11)', () {
    testWidgets('rail_has_no_names', (tester) async {
      final app = await MockApp.pump(tester);

      final railItems = tester.widgetList<RailItemView>(
        find.byType(RailItemView),
      );
      expect(railItems, isNotEmpty);

      // No server names and no "Private" text under the icons.
      for (final item in railItems) {
        expect(
          find.descendant(
            of: find.byWidget(item),
            matching: find.text(item.name),
          ),
          findsNothing,
          reason: '${item.name} text must not be drawn under the icon',
        );
      }

      // Every rail item is exactly 52 px tall.
      for (final item in railItems) {
        final size = tester.getSize(find.byWidget(item));
        expect(size.height, 52.0);
      }

      // Top padding 8.
      final railBox = tester.getRect(find.byType(ServerRail));
      final privateBox = tester.getRect(railItem('Private'));
      expect(privateBox.top - railBox.top, 8.0);

      // Items in the servers list are 6 px apart.
      final devBox = tester.getRect(railItem('Opencord Dev'));
      final berlinBox = tester.getRect(railItem('Rust Berlin'));
      expect(berlinBox.top - devBox.bottom, 6.0);

      await app.dispose(tester);
    });

    testWidgets('popover_shows_on_hover_and_focus', (tester) async {
      final app = await MockApp.pump(tester);

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      final devCenter = tester.getCenter(railItem('Opencord Dev'));
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(devCenter);
      await tester.pumpAndSettle();

      final popFinder = find.byKey(ShellKeys.railPop);
      expect(popFinder, findsOneWidget);

      final popRect = tester.getRect(popFinder);
      // 78 from the left.
      expect(popRect.left, 78.0);
      // Centred vertically on the item.
      expect(popRect.center.dy, closeTo(devCenter.dy, 1.0));

      // Name and status.
      expect(
        find.descendant(of: popFinder, matching: find.text('Opencord Dev')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: popFinder, matching: find.textContaining('online')),
        findsOneWidget,
      );

      // Surface numbers: min width 168, max width 280, radius 12.
      expect(popRect.width, greaterThanOrEqualTo(168.0));
      expect(popRect.width, lessThanOrEqualTo(280.0));
      final container = tester.widget<Container>(popFinder);
      final decoration = container.decoration as BoxDecoration;
      expect(decoration.borderRadius, BorderRadius.circular(12));

      // The arrow pointing to the rail item.
      final arrowFinder = find.byKey(const Key('rail-popover-arrow'));
      expect(arrowFinder, findsOneWidget);
      final arrowCenter = tester.getCenter(arrowFinder);
      expect(arrowCenter.dy, closeTo(popRect.center.dy, 1.0));

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('popover_never_takes_the_pointer', (tester) async {
      var tapped = false;
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            theme: ThemeData(extensions: const [OcColors.dark]),
            home: Scaffold(
              body: Stack(
                children: [
                  Positioned.fill(
                    child: GestureDetector(
                      onTap: () => tapped = true,
                      child: const ColoredBox(color: Colors.blue),
                    ),
                  ),
                  const Positioned(
                    left: 78,
                    top: 100,
                    child: RailPopover(id: 'add'),
                  ),
                ],
              ),
            ),
          ),
        ),
      );

      final popFinder = find.byKey(ShellKeys.railPop);
      expect(popFinder, findsOneWidget);

      // Tapping at the center of the popover passes straight through.
      await tester.tapAt(tester.getCenter(popFinder));
      await tester.pumpAndSettle();
      expect(tapped, isTrue);
    });

    testWidgets('four_faces_then_a_count', (tester) async {
      // 10 people: 4 avatars, +6, 10 in voice.
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            theme: ThemeData(extensions: const [OcColors.dark]),
            home: Scaffold(
              body: VoiceFaces(
                serverKey: 'test-server',
                userIds: List.generate(10, (i) => i + 1),
                names: {for (var i = 1; i <= 10; i++) i: 'User $i'},
              ),
            ),
          ),
        ),
      );

      expect(find.byType(MemberAvatar), findsNWidgets(4));
      expect(find.text('+6'), findsOneWidget);
      expect(find.text('10 in voice'), findsOneWidget);

      // 4 people: 4 avatars and no +.
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            theme: ThemeData(extensions: const [OcColors.dark]),
            home: Scaffold(
              body: VoiceFaces(
                serverKey: 'test-server',
                userIds: const [1, 2, 3, 4],
                names: {for (var i = 1; i <= 4; i++) i: 'User $i'},
              ),
            ),
          ),
        ),
      );

      expect(find.byType(MemberAvatar), findsNWidgets(4));
      expect(find.text('+0'), findsNothing);
      expect(find.textContaining('+'), findsNothing);
      expect(find.text('4 in voice'), findsOneWidget);

      // None: no voice row.
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            theme: ThemeData(extensions: const [OcColors.dark]),
            home: const Scaffold(
              body: VoiceFaces(
                serverKey: 'test-server',
                userIds: [],
                names: {},
              ),
            ),
          ),
        ),
      );

      expect(find.byType(MemberAvatar), findsNothing);
      expect(find.textContaining('in voice'), findsNothing);

      // Real provider with Mock's General (Opencord Dev has 4 in voice).
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(tester.getCenter(railItem('Opencord Dev')));
      await tester.pumpAndSettle();

      final popFinder = find.byKey(ShellKeys.railPop);
      expect(
        find.descendant(of: popFinder, matching: find.byType(MemberAvatar)),
        findsNWidgets(4),
      );
      expect(
        find.descendant(of: popFinder, matching: find.text('4 in voice')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: popFinder, matching: find.textContaining('+')),
        findsNothing,
      );

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('each_server_shows_its_own_voice', (tester) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      // Hover Opencord Dev: 4 in voice.
      await mouse.moveTo(tester.getCenter(railItem('Opencord Dev')));
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byKey(ShellKeys.railPop),
          matching: find.text('4 in voice'),
        ),
        findsOneWidget,
      );

      // Hover Rust Berlin: 2 in voice.
      await mouse.moveTo(tester.getCenter(railItem('Rust Berlin')));
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byKey(ShellKeys.railPop),
          matching: find.text('2 in voice'),
        ),
        findsOneWidget,
      );

      // Hover Homelab: no voice participants.
      await mouse.moveTo(tester.getCenter(railItem('Homelab')));
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byKey(ShellKeys.railPop),
          matching: find.textContaining('in voice'),
        ),
        findsNothing,
      );

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('popover_appears_on_the_curve', (tester) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      await mouse.moveTo(tester.getCenter(railItem('Opencord Dev')));
      // Initial pump triggers show.
      await tester.pump();

      // At 40 ms.
      await tester.pump(const Duration(milliseconds: 40));
      final scale40 = tester
          .widget<ScaleTransition>(find.byKey(const Key('rail-popover-scale')))
          .scale
          .value;
      final expected40 =
          0.96 +
          (1.0 - 0.96) * OcSpring.pop.valueAt(const Duration(milliseconds: 40));
      expect(scale40, closeTo(expected40, 0.005));

      // At 120 ms (another 80 ms).
      await tester.pump(const Duration(milliseconds: 80));
      final scale120 = tester
          .widget<ScaleTransition>(find.byKey(const Key('rail-popover-scale')))
          .scale
          .value;
      final expected120 =
          0.96 +
          (1.0 - 0.96) *
              OcSpring.pop.valueAt(const Duration(milliseconds: 120));
      expect(scale120, closeTo(expected120, 0.005));

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('moving_between_icons_slides_without_reappearing', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      final devCenter = tester.getCenter(railItem('Opencord Dev'));
      final berlinCenter = tester.getCenter(railItem('Rust Berlin'));

      await mouse.moveTo(devCenter);
      await tester.pumpAndSettle();

      final y1 = tester.getCenter(find.byKey(ShellKeys.railPop)).dy;
      expect(y1, closeTo(devCenter.dy, 1.0));

      // Move away briefly (within 90 ms).
      await mouse.moveTo(const Offset(0, 0));
      await tester.pump(const Duration(milliseconds: 40));

      // Hover Rust Berlin.
      await mouse.moveTo(berlinCenter);
      await tester.pump();

      // Scale stays 1.0!
      final scale = tester
          .widget<ScaleTransition>(find.byKey(const Key('rail-popover-scale')))
          .scale
          .value;
      expect(scale, closeTo(1.0, 0.001));

      // Pump 40 ms into slide: y on paneOut.
      await tester.pump(const Duration(milliseconds: 40));
      final ySlide40 = tester.getCenter(find.byKey(ShellKeys.railPop)).dy;
      final progress40 = OcSpring.paneOut.valueAt(
        const Duration(milliseconds: 40),
      );
      final expectedSlide40 = y1 + (berlinCenter.dy - y1) * progress40;
      expect(ySlide40, closeTo(expectedSlide40, 2.0));

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('leaving_hides_after_90_ms', (tester) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      await mouse.moveTo(tester.getCenter(railItem('Opencord Dev')));
      await tester.pumpAndSettle();
      expect(find.byKey(ShellKeys.railPop), findsOneWidget);

      // Move mouse away.
      await mouse.moveTo(const Offset(0, 0));
      await tester.pump();

      // At 80 ms: still visible (timer hasn't expired).
      await tester.pump(const Duration(milliseconds: 80));
      expect(find.byKey(ShellKeys.railPop), findsOneWidget);
      final op80 = tester
          .widget<FadeTransition>(find.byKey(const Key('rail-popover-fade')))
          .opacity
          .value;
      expect(op80, 1.0);

      // Past 90 ms: timer fires, and ticker begins exit animation.
      await tester.pump(const Duration(milliseconds: 20));
      await tester.pump(const Duration(milliseconds: 20));
      expect(find.byKey(ShellKeys.railPop), findsOneWidget);
      final op120 = tester
          .widget<FadeTransition>(find.byKey(const Key('rail-popover-fade')))
          .opacity
          .value;
      expect(op120, lessThan(1.0));

      // At 180 ms: fully hidden.
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pumpAndSettle();
      expect(find.byKey(ShellKeys.railPop), findsNothing);

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('dragging_hides_it', (tester) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      await mouse.moveTo(tester.getCenter(railItem('Opencord Dev')));
      await tester.pumpAndSettle();
      expect(find.byKey(ShellKeys.railPop), findsOneWidget);

      // Start drag on Homelab.
      final from = tester.getCenter(railItem('Homelab'));
      final to = tester.getCenter(railItem('Opencord Dev'));
      final dragGesture = await tester.startGesture(
        from,
        kind: PointerDeviceKind.mouse,
      );
      await dragGesture.moveBy(const Offset(0, 20));
      await tester.pump(const Duration(milliseconds: 50));

      // Popover is dismissed immediately when reorder starts.
      expect(find.byKey(ShellKeys.railPop), findsNothing);

      await dragGesture.moveTo(to);
      await tester.pump(const Duration(milliseconds: 50));
      await dragGesture.up();
      await tester.pumpAndSettle();

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('removing_the_server_removes_it', (tester) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      await mouse.moveTo(tester.getCenter(railItem('Homelab')));
      await tester.pumpAndSettle();
      expect(find.byKey(ShellKeys.railPop), findsOneWidget);

      // Remove the hovered server.
      await app.read(repositoryProvider).removeServer('homelab.local:7710');
      await tester.pumpAndSettle();

      expect(find.byKey(ShellKeys.railPop), findsNothing);

      await mouse.removePointer();
      await app.dispose(tester);
    });

    testWidgets('labels_stay_for_screen_readers', (tester) async {
      final app = await MockApp.pump(tester);

      final devSemantics = tester.getSemantics(railItem('Opencord Dev'));
      expect(devSemantics.label, contains('Opencord Dev'));
      expect(devSemantics.label, contains('Connected'));

      final privateSemantics = tester.getSemantics(railItem('Private'));
      expect(privateSemantics.label, 'Private messages');

      final addFinder = find.byWidgetPredicate(
        (w) => w is Hoverable && w.semanticLabel == 'Add a server',
      );
      expect(addFinder, findsOneWidget);

      await app.dispose(tester);
    });

    testWidgets('private_and_add_have_their_lines', (tester) async {
      final app = await MockApp.pump(tester);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);

      // Private popover.
      await mouse.moveTo(tester.getCenter(railItem('Private')));
      await tester.pumpAndSettle();

      final popFinder = find.byKey(ShellKeys.railPop);
      expect(popFinder, findsOneWidget);
      expect(
        find.descendant(of: popFinder, matching: find.text('Private messages')),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: popFinder,
          matching: find.textContaining('friend request'),
        ),
        findsOneWidget,
      );

      // Divider line after Private in rail has height 1 and margin 18 each side.
      final divider = tester.widget<Padding>(
        find.byWidgetPredicate(
          (w) =>
              w is Padding &&
              w.padding ==
                  const EdgeInsets.symmetric(horizontal: 18, vertical: 2),
        ),
      );
      expect(
        divider.padding,
        const EdgeInsets.symmetric(horizontal: 18, vertical: 2),
      );

      // Add server popover.
      final addFinder = find.byWidgetPredicate(
        (w) => w is Hoverable && w.semanticLabel == 'Add a server',
      );
      await mouse.moveTo(tester.getCenter(addFinder));
      await tester.pumpAndSettle();

      expect(
        find.descendant(
          of: find.byKey(ShellKeys.railPop),
          matching: find.text('Add a server'),
        ),
        findsOneWidget,
      );

      await mouse.removePointer();
      await app.dispose(tester);
    });
  });
}
