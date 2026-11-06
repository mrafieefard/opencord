import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';

import '../../support/pump.dart';

Widget _launcher(
  void Function(BuildContext context) action, {
  bool reducedMotion = false,
}) => MediaQuery(
  data: MediaQueryData(
    size: const Size(800, 600),
    disableAnimations: reducedMotion,
  ),
  child: Center(
    child: Builder(
      builder: (context) =>
          OcButton(label: 'Open', onPressed: () => action(context)),
    ),
  ),
);

void main() {
  group('Task 20: Menus, popovers, dialogs', () {
    testWidgets('menu_numbers', (tester) async {
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(400, 300),
        Center(
          child: OcMenuPanel(
            entries: [
              OcMenuItem(
                label: 'Item 1',
                icon: OcIcons.reply,
                onSelected: () {},
              ),
              OcMenuDivider(),
              OcMenuItem(label: 'Item 2', onSelected: () {}),
            ],
          ),
        ),
      );

      // Surface
      final panelContainer = tester.widget<Container>(
        find.descendant(
          of: find.byType(OcMenuPanel),
          matching: find.byType(Container).first,
        ),
      );
      final boxConstraints = panelContainer.constraints!;
      expect(boxConstraints.minWidth, 190);
      expect(boxConstraints.maxWidth, 300);
      expect(panelContainer.padding, const EdgeInsets.all(4));
      final decoration = panelContainer.decoration! as BoxDecoration;
      expect(decoration.color, OcColors.dark.elevated);
      expect(decoration.borderRadius, BorderRadius.circular(10));
      expect(decoration.border, Border.all(color: OcColors.dark.border));
      expect(decoration.boxShadow, OcShadows.menu(OcColors.dark));

      // Item 1
      final itemContainers = tester
          .widgetList<Container>(
            find.descendant(
              of: find.byType(OcMenuPanel),
              matching: find.byWidgetPredicate(
                (w) =>
                    w is Container &&
                    w.constraints?.maxHeight == 32 &&
                    w.decoration is BoxDecoration &&
                    (w.decoration as BoxDecoration).borderRadius ==
                        BorderRadius.circular(6),
              ),
            ),
          )
          .toList();

      expect(itemContainers.isNotEmpty, isTrue);
      final item1Container = itemContainers.first;
      expect(item1Container.constraints?.maxHeight, 32);
      expect(
        item1Container.padding,
        const EdgeInsets.symmetric(horizontal: 10),
      );

      final textWidget = tester.widget<Text>(find.text('Item 1'));
      expect(textWidget.style?.fontSize, 13.5);

      final iconWidget = tester.widget<Icon>(find.byIcon(OcIcons.reply));
      expect(iconWidget.size, 18);
      expect(iconWidget.color, OcColors.dark.textSecondary);

      // Divider margin 4 6 (vertical: 4, horizontal: 6)
      final dividerPadding = tester.widget<Padding>(
        find.byWidgetPredicate(
          (w) =>
              w is Padding &&
              w.padding ==
                  const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
        ),
      );
      expect(
        dividerPadding.padding,
        const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
      );
      final dividerBox = tester.widget<SizedBox>(
        find.descendant(
          of: find.byWidget(dividerPadding),
          matching: find.byType(SizedBox),
        ),
      );
      expect(dividerBox.height, 1);
    });

    testWidgets('menu_opens_on_the_curve', (tester) async {
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showOcMenu(
            context: context,
            position: const Offset(100, 100),
            entries: [OcMenuItem(label: 'Item 1', onSelected: () {})],
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pump(); // t = 0

      double getScale() => tester
          .widget<ScaleTransition>(find.byType(ScaleTransition))
          .scale
          .value;
      double getOpacity() => tester
          .widget<FadeTransition>(find.byType(FadeTransition))
          .opacity
          .value;
      double getOffsetY() {
        final transform = tester.widget<Transform>(
          find.byType(Transform).first,
        );
        return transform.transform.storage[13];
      }

      expect(getScale(), closeTo(0.97, 0.005));
      expect(getOpacity(), closeTo(0.0, 0.02));
      expect(getOffsetY(), closeTo(-2.0, 0.1));

      // At t = 40 ms
      await tester.pump(const Duration(milliseconds: 40));
      expect(getScale(), closeTo(0.981, 0.005));
      expect(getOpacity(), closeTo(0.38, 0.03));
      expect(getOffsetY(), closeTo(-1.25, 0.1));

      // At t = 80 ms
      await tester.pump(const Duration(milliseconds: 40));
      expect(getScale(), closeTo(0.993, 0.005));
      expect(getOpacity(), closeTo(0.76, 0.03));
      expect(getOffsetY(), closeTo(-0.48, 0.1));

      // At t = 120 ms
      await tester.pump(const Duration(milliseconds: 40));
      expect(getScale(), closeTo(0.998, 0.005));
      expect(getOpacity(), closeTo(0.93, 0.03));
      expect(getOffsetY(), closeTo(-0.13, 0.1));

      // At t = 160 ms
      await tester.pump(const Duration(milliseconds: 40));
      expect(getScale(), closeTo(1.0, 0.005));
      expect(getOpacity(), closeTo(0.99, 0.03));
      expect(getOffsetY(), closeTo(-0.02, 0.1));

      // At t = 200 ms
      await tester.pump(const Duration(milliseconds: 40));
      expect(getScale(), closeTo(1.0, 0.005));
      expect(getOpacity(), closeTo(1.0, 0.02));
      expect(getOffsetY(), closeTo(0.0, 0.05));
    });

    testWidgets('it_grows_from_the_corner_nearest_the_trigger', (tester) async {
      late BuildContext rootContext;
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        Builder(
          builder: (context) {
            rootContext = context;
            return const SizedBox.expand();
          },
        ),
      );

      // Mid-way, the corner nearest the click is where it ends, apart from
      // the rise; the rest of the menu is still growing towards its place.
      Future<void> growsFrom(Finder popup, Alignment corner, String at) async {
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 40));
        final mid = tester.getRect(popup);
        await tester.pumpAndSettle();
        final end = tester.getRect(popup);
        final from = corner.withinRect(mid) - corner.withinRect(end);
        expect(from.dx, moreOrLessEquals(0, epsilon: 0.01), reason: at);
        expect(from.dy, inInclusiveRange(-2, 0), reason: at);
        expect(mid.width, lessThan(end.width - 1), reason: at);
        Navigator.pop(rootContext);
        await tester.pump();
      }

      final clicks = [
        (const Offset(100, 100), Alignment.topLeft),
        // Too near the right edge: it opens to the click's left.
        (const Offset(700, 100), Alignment.topRight),
        // In the window's lower half, it still opens below the click.
        (const Offset(100, 500), Alignment.topLeft),
        (const Offset(700, 500), Alignment.topRight),
        // Too near the bottom: pushed up, so the click is on its lower half.
        (const Offset(100, 585), Alignment.bottomLeft),
        (const Offset(700, 585), Alignment.bottomRight),
      ];
      for (final (click, corner) in clicks) {
        showOcMenu(
          context: rootContext,
          position: click,
          entries: [OcMenuItem(label: 'Choice', onSelected: () {})],
        );
        await growsFrom(find.byType(OcMenuPanel), corner, 'menu at $click');
      }

      // A popover that flips above its anchor grows from its lower corner.
      showPopover(
        context: rootContext,
        anchor: const Rect.fromLTWH(200, 560, 40, 40),
        side: PopoverSide.below,
        builder: (context) => const SizedBox(width: 100, height: 100),
      );
      await growsFrom(
        find.byType(PopoverPanel),
        Alignment.bottomLeft,
        'popover flipped above',
      );
    });

    testWidgets('closing_is_instant_and_the_action_still_runs', (tester) async {
      var actionRan = 0;
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showOcMenu(
            context: context,
            position: const Offset(200, 200),
            entries: [
              OcMenuItem(label: 'Action', onSelected: () => actionRan++),
            ],
          );
        }),
      );

      // First run
      await tester.tap(find.text('Open'));
      await tester.pump();
      await tester.pump(OcSpring.pop.settle);
      expect(find.byType(OcMenuPanel), findsOneWidget);

      await tester.tap(find.text('Action'));
      await tester.pump(); // exactly one frame
      expect(
        find.byType(OcMenuPanel),
        findsNothing,
        reason: 'gone on next frame',
      );
      expect(actionRan, 1);

      // Second run (twice over)
      await tester.tap(find.text('Open'));
      await tester.pump();
      await tester.pump(OcSpring.pop.settle);
      expect(find.byType(OcMenuPanel), findsOneWidget);

      await tester.tap(find.text('Action'));
      await tester.pump(); // exactly one frame
      expect(
        find.byType(OcMenuPanel),
        findsNothing,
        reason: 'gone on next frame',
      );
      expect(actionRan, 2);
    });

    testWidgets('keyboard_still_works', (tester) async {
      var chosen = '';
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showOcMenu(
            context: context,
            position: const Offset(200, 200),
            entries: [
              OcMenuItem(label: 'One', onSelected: () => chosen = 'one'),
              OcMenuItem(label: 'Two', onSelected: () => chosen = 'two'),
            ],
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(chosen, 'two');
      expect(find.byType(OcMenuPanel), findsNothing);
    });

    testWidgets('submenu_still_works', (tester) async {
      var subChosen = '';
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showOcMenu(
            context: context,
            position: const Offset(200, 200),
            entries: [
              OcMenuItem(
                label: 'More',
                submenu: [
                  OcMenuItem(
                    label: 'SubAction',
                    onSelected: () => subChosen = 'sub',
                  ),
                ],
              ),
            ],
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('More'));
      await tester.pumpAndSettle();
      expect(find.text('SubAction'), findsOneWidget);

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await tester.pump();
      expect(find.text('SubAction'), findsNothing);
      expect(find.text('More'), findsOneWidget);

      await tester.tap(find.text('More'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('SubAction'));
      await tester.pump();

      expect(subChosen, 'sub');
      expect(find.byType(OcMenuPanel), findsNothing);
    });

    testWidgets('popover_opens_the_same_way', (tester) async {
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showPopover(
            context: context,
            anchor: const Rect.fromLTWH(200, 200, 40, 40),
            builder: (context) => const Text('Popover content'),
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pump(); // t = 0

      final scaleTransition = tester.widget<ScaleTransition>(
        find.byType(ScaleTransition),
      );
      expect(scaleTransition.scale.value, closeTo(0.97, 0.005));
      final fadeTransition = tester.widget<FadeTransition>(
        find.byType(FadeTransition),
      );
      expect(fadeTransition.opacity.value, closeTo(0.0, 0.02));

      await tester.pump(OcSpring.pop.settle);
      expect(scaleTransition.scale.value, closeTo(1.0, 0.005));
      expect(fadeTransition.opacity.value, closeTo(1.0, 0.005));
      expect(find.text('Popover content'), findsOneWidget);
    });

    testWidgets('popover_flips_as_today', (tester) async {
      late BuildContext rootContext;
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        Builder(
          builder: (context) {
            rootContext = context;
            return const SizedBox.expand();
          },
        ),
      );

      // 1. Below flips to above near bottom edge
      showPopover(
        context: rootContext,
        anchor: const Rect.fromLTWH(200, 560, 40, 40),
        side: PopoverSide.below,
        builder: (context) =>
            const SizedBox(width: 100, height: 100, child: Text('Flip1')),
      );
      await tester.pumpAndSettle();
      final pos1 = tester.getTopLeft(find.text('Flip1'));
      expect(pos1.dy, lessThan(560));
      Navigator.pop(rootContext);
      await tester.pumpAndSettle();

      // 2. Above flips to below near top edge
      showPopover(
        context: rootContext,
        anchor: const Rect.fromLTWH(200, 10, 40, 40),
        side: PopoverSide.above,
        builder: (context) =>
            const SizedBox(width: 100, height: 100, child: Text('Flip2')),
      );
      await tester.pumpAndSettle();
      final pos2 = tester.getTopLeft(find.text('Flip2'));
      expect(pos2.dy, greaterThanOrEqualTo(50));
      Navigator.pop(rootContext);
      await tester.pumpAndSettle();

      // 3. Right flips to left near right edge
      showPopover(
        context: rootContext,
        anchor: const Rect.fromLTWH(750, 200, 40, 40),
        side: PopoverSide.right,
        builder: (context) =>
            const SizedBox(width: 100, height: 100, child: Text('Flip3')),
      );
      await tester.pumpAndSettle();
      final pos3 = tester.getTopLeft(find.text('Flip3'));
      expect(pos3.dx, lessThan(750));
      Navigator.pop(rootContext);
      await tester.pumpAndSettle();
    });

    testWidgets('dialog_opens_on_pop', (tester) async {
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showOcDialog(
            context: context,
            builder: (context) => const OcDialog(
              title: 'Test Dialog',
              child: Text('Dialog body'),
            ),
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pump(); // t = 0

      final scaleTransition = tester.widget<ScaleTransition>(
        find.byType(ScaleTransition),
      );
      expect(scaleTransition.scale.value, closeTo(0.97, 0.005));
      final fadeTransition = tester.widget<FadeTransition>(
        find.byType(FadeTransition),
      );
      expect(fadeTransition.opacity.value, closeTo(0.0, 0.02));

      await tester.pump(OcSpring.pop.settle);
      expect(scaleTransition.scale.value, closeTo(1.0, 0.005));
      expect(fadeTransition.opacity.value, closeTo(1.0, 0.005));
      expect(find.text('Test Dialog'), findsOneWidget);
    });

    testWidgets('closes_in_100ms', (tester) async {
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        _launcher((context) {
          showOcDialog(
            context: context,
            builder: (context) => OcDialog(
              title: 'Closing Dialog',
              actions: [
                OcButton(
                  label: 'Cancel',
                  onPressed: () => Navigator.pop(context),
                ),
              ],
              child: const Text('Body'),
            ),
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      expect(find.byType(OcDialog), findsOneWidget);

      await tester.tap(find.text('Cancel'));
      await tester.pump(); // Closing starts

      // At 50 ms: dialog is still visible, fading out
      await tester.pump(const Duration(milliseconds: 50));
      expect(find.byType(OcDialog), findsOneWidget);
      final fade = tester.widget<FadeTransition>(find.byType(FadeTransition));
      expect(fade.opacity.value, lessThan(1.0));
      expect(fade.opacity.value, greaterThan(0.0));

      // At 100 ms total: dialog is gone
      await tester.pump(const Duration(milliseconds: 60));
      expect(find.byType(OcDialog), findsNothing);
    });

    testWidgets('reduced_motion_is_instant', (tester) async {
      await pumpThemed(
        tester,
        colors: OcColors.dark,
        surface: const Size(800, 600),
        MediaQuery(
          data: const MediaQueryData(
            size: Size(800, 600),
            disableAnimations: true,
          ),
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Builder(
                  builder: (context) => GestureDetector(
                    onTap: () => showOcMenu(
                      context: context,
                      position: const Offset(200, 200),
                      entries: [
                        OcMenuItem(label: 'Instant Menu', onSelected: () {}),
                      ],
                    ),
                    child: const Text('Open Menu'),
                  ),
                ),
                const SizedBox(width: 20),
                Builder(
                  builder: (context) => GestureDetector(
                    onTap: () => showOcDialog(
                      context: context,
                      builder: (context) => const OcDialog(
                        title: 'Instant Dialog',
                        child: Text('Content'),
                      ),
                    ),
                    child: const Text('Open Dialog'),
                  ),
                ),
              ],
            ),
          ),
        ),
      );

      // Open menu
      await tester.tap(find.text('Open Menu'));
      await tester.pump();
      expect(find.text('Instant Menu'), findsOneWidget);
      expect(tester.hasRunningAnimations, isFalse);

      // Close menu
      await tester.tap(find.text('Instant Menu'));
      await tester.pump();
      expect(find.text('Instant Menu'), findsNothing);
      expect(tester.hasRunningAnimations, isFalse);

      // Open dialog
      await tester.tap(find.text('Open Dialog'));
      await tester.pump();
      expect(find.text('Instant Dialog'), findsOneWidget);
      expect(tester.hasRunningAnimations, isFalse);

      // Close dialog
      await tester.tapAt(const Offset(10, 10)); // scrim click
      await tester.pump();
      expect(find.text('Instant Dialog'), findsNothing);
      expect(tester.hasRunningAnimations, isFalse);
    });
  });
}
