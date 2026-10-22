import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

import '../../support/pump.dart';

/// A button that runs [action] with a context below the app's navigator.
Widget launcher(void Function(BuildContext context) action) => Center(
  child: Builder(
    builder: (context) =>
        OcButton(label: 'Open', onPressed: () => action(context)),
  ),
);

void main() {
  group('dialogs', () {
    testWidgets('show a title and actions, and return the chosen value', (
      tester,
    ) async {
      Future<String?>? result;
      await pumpThemed(
        tester,
        launcher((context) {
          result = showOcDialog<String>(
            context: context,
            builder: (context) => OcDialog(
              title: 'Delete channel',
              actions: [
                OcButton(
                  label: 'Cancel',
                  onPressed: () => Navigator.pop(context),
                ),
                OcButton.primary(
                  label: 'Delete channel',
                  onPressed: () => Navigator.pop(context, 'deleted'),
                ),
              ],
              child: const Text('This cannot be undone.'),
            ),
          );
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      expect(find.text('This cannot be undone.'), findsOneWidget);
      await tester.tap(find.widgetWithText(OcButton, 'Delete channel'));
      await tester.pumpAndSettle();

      expect(await result, 'deleted');
      expect(find.text('This cannot be undone.'), findsNothing);
    });

    testWidgets('close on Escape and on a click outside', (tester) async {
      await pumpThemed(
        tester,
        launcher(
          (context) => showOcDialog<void>(
            context: context,
            builder: (context) =>
                const OcDialog(title: 'Invite people', child: Text('Link')),
          ),
        ),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      final closedByEscape = find.text('Invite people').evaluate().isEmpty;
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tapAt(const Offset(4, 4));
      await tester.pumpAndSettle();

      expect(closedByEscape, isTrue);
      expect(find.text('Invite people'), findsNothing);
    });
  });

  group('menus', () {
    late List<String> chosen;

    List<OcMenuEntry> entries() => [
      OcMenuItem(
        label: 'Reply',
        icon: OcIcons.reply,
        onSelected: () => chosen.add('reply'),
      ),
      OcMenuItem(
        label: 'Copy text',
        icon: OcIcons.contentCopy,
        shortcut: const SingleActivator(LogicalKeyboardKey.keyC, control: true),
        onSelected: () => chosen.add('copy'),
      ),
      const OcMenuItem(label: 'Pin', icon: OcIcons.pushPin),
      OcMenuItem(
        label: 'Roles',
        submenu: [
          OcMenuItem(
            label: 'Moderator',
            checked: true,
            onSelected: () => chosen.add('moderator'),
          ),
        ],
      ),
      const OcMenuDivider(),
      OcMenuItem(label: 'Delete', onSelected: () => chosen.add('delete')),
    ];

    Future<void> openMenu(WidgetTester tester) async {
      chosen = [];
      await pumpThemed(
        tester,
        launcher(
          (context) => showOcMenu(
            context: context,
            position: const Offset(200, 150),
            entries: entries(),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
    }

    testWidgets('open at the given position with shortcut hints', (
      tester,
    ) async {
      await openMenu(tester);

      expect(
        tester.getTopLeft(find.byType(OcMenuPanel)),
        const Offset(200, 150),
      );
      expect(find.text('Ctrl+C'), findsOneWidget);
    });

    testWidgets('choosing an item closes the menu and runs it', (tester) async {
      await openMenu(tester);

      await tester.tap(find.text('Reply'));
      await tester.pumpAndSettle();

      expect(chosen, ['reply']);
      expect(find.byType(OcMenuPanel), findsNothing);
    });

    testWidgets('disabled items do nothing', (tester) async {
      await openMenu(tester);

      await tester.tap(find.text('Pin'));
      await tester.pumpAndSettle();

      expect(chosen, isEmpty);
      expect(find.byType(OcMenuPanel), findsOneWidget);
    });

    testWidgets('work from the keyboard', (tester) async {
      await openMenu(tester);

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();

      expect(chosen, ['copy']);
    });

    testWidgets('Escape closes without choosing', (tester) async {
      await openMenu(tester);

      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      expect(chosen, isEmpty);
      expect(find.byType(OcMenuPanel), findsNothing);
    });

    testWidgets('submenus open beside their item', (tester) async {
      await openMenu(tester);

      await tester.tap(find.text('Roles'));
      await tester.pumpAndSettle();
      final parent = tester.getRect(find.byType(OcMenuPanel).first);
      final child = tester.getRect(find.byType(OcMenuPanel).last);
      await tester.tap(find.text('Moderator'));
      await tester.pumpAndSettle();

      expect(child.left, greaterThanOrEqualTo(parent.right - 1));
      expect(chosen, ['moderator']);
      expect(find.byType(OcMenuPanel), findsNothing);
    });

    testWidgets('stay on screen near the edges', (tester) async {
      chosen = [];
      await pumpThemed(
        tester,
        launcher(
          (context) => showOcMenu(
            context: context,
            position: const Offset(790, 590),
            entries: entries(),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      final rect = tester.getRect(find.byType(OcMenuPanel));
      expect(rect.right, lessThanOrEqualTo(800));
      expect(rect.bottom, lessThanOrEqualTo(600));
    });
  });

  group('popovers', () {
    testWidgets('open below their anchor and close on Escape', (tester) async {
      await pumpThemed(
        tester,
        Center(
          child: Builder(
            builder: (context) => OcButton(
              label: 'Anchor',
              onPressed: () => showPopover<void>(
                context: context,
                anchor: globalRectOf(context),
                builder: (context) => const Text('Popover body'),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('Anchor'));
      await tester.pumpAndSettle();
      final anchor = tester.getRect(find.byType(OcButton));
      final body = tester.getRect(find.byType(PopoverPanel));
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      expect(body.top, greaterThan(anchor.bottom));
      expect(find.text('Popover body'), findsNothing);
    });
  });

  group('toasts', () {
    testWidgets('show one at a time and fade after 1.8 s', (tester) async {
      await pumpThemed(
        tester,
        launcher((context) {
          showOcToast(context, 'Copied');
          showOcToast(context, 'Invite link copied');
        }),
      );

      await tester.tap(find.text('Open'));
      await tester.pump(const Duration(milliseconds: 200));
      final visibleAtFirst =
          find.text('Copied').evaluate().isEmpty &&
          find.text('Invite link copied').evaluate().isNotEmpty;
      await tester.pump(const Duration(milliseconds: 1800));
      await tester.pumpAndSettle();

      expect(visibleAtFirst, isTrue);
      expect(find.text('Invite link copied'), findsNothing);
    });

    testWidgets('can offer an undo', (tester) async {
      var undone = false;
      await pumpThemed(
        tester,
        launcher(
          (context) => showOcToast(
            context,
            'Reaction removed',
            actionLabel: 'Undo',
            onAction: () => undone = true,
          ),
        ),
      );

      await tester.tap(find.text('Open'));
      await tester.pump(const Duration(milliseconds: 200));
      await tester.tap(find.text('Undo'));
      await tester.pumpAndSettle();

      expect(undone, isTrue);
      expect(find.text('Reaction removed'), findsNothing);
    });
  });
}
