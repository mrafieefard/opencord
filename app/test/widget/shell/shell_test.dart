import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/window_title.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

import '../../support/app.dart';

Finder column(Key key) => find.byKey(key);

double widthOf(WidgetTester tester, Key key) =>
    tester.getSize(column(key)).width;

Future<void> press(
  WidgetTester tester,
  LogicalKeyboardKey key, {
  bool control = false,
  bool alt = false,
  bool shift = false,
}) async {
  final modifiers = [
    if (control) LogicalKeyboardKey.controlLeft,
    if (alt) LogicalKeyboardKey.altLeft,
    if (shift) LogicalKeyboardKey.shiftLeft,
  ];
  for (final modifier in modifiers) {
    await tester.sendKeyDownEvent(modifier);
  }
  await tester.sendKeyEvent(key);
  for (final modifier in modifiers.reversed) {
    await tester.sendKeyUpEvent(modifier);
  }
  await tester.pump();
}

void main() {
  testWidgets('four columns at 1440 px', (tester) async {
    final app = await MockApp.pump(tester);

    expect(widthOf(tester, DesktopShell.railKey), OcSize.railWidth);
    expect(widthOf(tester, DesktopShell.sidebarKey), OcSize.sidebarWidth);
    expect(widthOf(tester, DesktopShell.membersKey), OcSize.memberPanelWidth);
    expect(column(DesktopShell.mainKey), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('below 1180 px the member list opens as a drawer', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1100, 800));
    final hidden = column(DesktopShell.membersKey).evaluate().isEmpty;

    await press(tester, LogicalKeyboardKey.keyU, control: true, shift: true);
    await tester.pumpAndSettle();
    final drawer = tester.getRect(column(DesktopShell.membersKey));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();

    expect(hidden, isTrue);
    expect(drawer.right, 1100);
    expect(column(DesktopShell.membersKey), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('below 980 px the sidebar narrows to 264 px', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(900, 700));

    expect(widthOf(tester, DesktopShell.sidebarKey), OcSize.sidebarNarrowWidth);
    await app.dispose(tester);
  });

  testWidgets('below 760 px the sidebar opens from a menu button', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(700, 600));
    final hidden = column(DesktopShell.sidebarKey).evaluate().isEmpty;

    await tester.tap(find.bySemanticsLabel('Channels'));
    await tester.pumpAndSettle();

    expect(hidden, isTrue);
    expect(
      tester.getRect(column(DesktopShell.sidebarKey)).left,
      OcSize.railWidth + 1,
    );
    await app.dispose(tester);
  });

  testWidgets('Ctrl+Alt+arrows switch servers, Alt+arrows switch channels', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final servers = app.read(serverListProvider);
    final firstChannel = app.read(currentChannelProvider);

    await press(tester, LogicalKeyboardKey.arrowDown, control: true, alt: true);
    final secondServer = app.read(currentServerProvider);
    await press(tester, LogicalKeyboardKey.arrowUp, control: true, alt: true);
    await press(tester, LogicalKeyboardKey.arrowDown, alt: true);
    final nextChannel = app.read(currentChannelProvider);

    expect(secondServer, servers[1].key);
    expect(app.read(currentServerProvider), servers[0].key);
    expect(nextChannel, isNot(firstChannel));
    await app.dispose(tester);
  });

  testWidgets('Alt+Shift+Down jumps to the next unread channel', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final server = app.read(currentServerProvider)!;
    final start = app.read(currentChannelProvider)!;
    final channels = app.read(serverProvider(server)).data!.channels.values;
    final order = [
      for (final channel in navigableChannels(channels)) channel.id,
    ];
    final activity = app.read(activityProvider(server));
    final expected = neighborWhere(
      order,
      start,
      1,
      (id) => activity.of(id).unread,
    );

    await press(tester, LogicalKeyboardKey.arrowDown, alt: true, shift: true);

    expect(expected, isNotNull);
    expect(app.read(currentChannelProvider), expected);
    await app.dispose(tester);
  });

  testWidgets('Ctrl+Shift+M and Ctrl+Shift+D toggle mute and deafen', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    await press(tester, LogicalKeyboardKey.keyM, control: true, shift: true);
    final muted = app.read(voiceSessionProvider).muted;
    await press(tester, LogicalKeyboardKey.keyD, control: true, shift: true);
    final deafened = app.read(voiceSessionProvider).deafened;
    await press(tester, LogicalKeyboardKey.keyD, control: true, shift: true);

    expect(muted, isTrue);
    expect(deafened, isTrue);
    expect(
      app.read(voiceSessionProvider).muted,
      isTrue,
      reason: 'undeafening restores the earlier mute',
    );
    await app.dispose(tester);
  });

  testWidgets('the window title names the channel and the unread count', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    expect(
      app.read(windowTitleProvider),
      matches(RegExp(r'^\(\d+\) #\S+ · Opencord Dev — Opencord$')),
    );
    await app.dispose(tester);
  });

  for (final size in const [Size(1440, 900), Size(1180, 800), Size(900, 700)]) {
    for (final scale in const [1.0, 1.5]) {
      testWidgets(
        'no overflow at ${size.width.toInt()}×${size.height.toInt()}, text ×$scale',
        (tester) async {
          final app = await MockApp.pump(tester, size: size, textScale: scale);

          expect(tester.takeException(), isNull);
          await app.dispose(tester);
        },
      );
    }
  }
}
