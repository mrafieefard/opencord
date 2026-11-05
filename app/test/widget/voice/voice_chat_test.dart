import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/shortcuts.dart';
import 'package:opencord/features/voice/voice_chat_slot.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/features/voice/voice_view.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

int _voiceChannel(MockApp app) {
  final channels = app.read(serverProvider(_dev)).data!.channels.values;
  return channels.firstWhere((c) => c.kind == ChannelKind.voice).id;
}

Future<void> _joinGeneral(WidgetTester tester, MockApp app) async {
  final voice = _voiceChannel(app);
  app.read(navigationProvider.notifier).openChannel(_dev, voice);
  await tester.pumpAndSettle();
  await tester.tap(find.widgetWithText(OcButton, 'Join voice'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('chat_beside_the_tiles_when_there_is_room', (tester) async {
    // at 1600: slot 380, its header, list and composer float, bottom level with the user panel;
    // main keeps its tiles; opens on pane.
    final app = await MockApp.pump(tester, size: const Size(1600, 800));
    await _joinGeneral(tester, app);

    // Initial state: 6 tiles shown in main, side slot does not have voice chat
    expect(find.byType(VoiceTileView), findsNWidgets(6));
    expect(find.byType(VoiceChatSlot), findsNothing);

    // Tap Show chat in the dock
    await tester.tap(find.byType(VoiceChatButton));
    await tester.pump();

    // Opening on pane: slot is expanding
    await tester.pump(const Duration(milliseconds: 100));
    final slotBoxMoving = tester.getRect(find.byKey(ShellKeys.side));
    expect(slotBoxMoving.width, greaterThan(0.0));
    expect(slotBoxMoving.width, lessThan(380.0));

    await tester.pumpAndSettle();

    // Settled at 380 wide
    final slotBox = tester.getRect(find.byKey(ShellKeys.side));
    expect(slotBox.width, 380.0);

    // Header, list and composer float exist in side slot
    expect(
      find.descendant(
        of: find.byKey(ShellKeys.side),
        matching: find.text('Chat · General'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byKey(ShellKeys.side),
        matching: find.byType(MessageList),
      ),
      findsOneWidget,
    );
    final composerFinder = find.descendant(
      of: find.byKey(ShellKeys.side),
      matching: find.byKey(ShellKeys.composer),
    );
    expect(composerFinder, findsOneWidget);

    // Bottom level with the user panel
    final userPanelFloat = tester.getRect(
      find.ancestor(of: find.byType(UserPanel), matching: find.byType(OcFloat)).first,
    );
    final composerRect = tester.getRect(composerFinder);
    expect(composerRect.bottom, userPanelFloat.bottom);

    // Main keeps its tiles
    expect(find.byType(VoiceTileView), findsNWidgets(6));

    await app.dispose(tester);
  });

  testWidgets('chat_replaces_the_tiles_when_there_is_not', (tester) async {
    // at 1280: main holds the chat; composer float beneath.
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _joinGeneral(tester, app);

    // Tap Show chat in the dock
    await tester.tap(find.byType(VoiceChatButton));
    await tester.pumpAndSettle();

    // Side slot is not open (width is 0, no VoiceChatSlot)
    final slotBox = tester.getRect(find.byKey(ShellKeys.side));
    expect(slotBox.width, 0.0);
    expect(find.byType(VoiceChatSlot), findsNothing);

    // Main holds the chat
    expect(
      find.descendant(
        of: find.byKey(ShellKeys.main),
        matching: find.byType(MessageList),
      ),
      findsOneWidget,
    );
    expect(find.byType(VoiceTileView), findsNothing);

    // Composer float beneath main
    final composerFinder = find.byKey(ShellKeys.composer);
    expect(composerFinder, findsOneWidget);
    expect(
      find.descendant(
        of: find.byKey(ShellKeys.main),
        matching: find.byType(Composer),
      ),
      findsNothing,
    );
    final mainRect = tester.getRect(find.byKey(ShellKeys.main));
    final composerRect = tester.getRect(composerFinder);
    expect(composerRect.top, mainRect.bottom + 8.0);

    await app.dispose(tester);
  });

  testWidgets('opening_the_right_pane_closes_the_chat', (tester) async {
    // opening_the_right_pane_closes_the_chat, as today, and the reverse.
    final app = await MockApp.pump(tester, size: const Size(1600, 800));
    await _joinGeneral(tester, app);

    // Open chat
    await tester.tap(find.byType(VoiceChatButton));
    await tester.pumpAndSettle();
    expect(find.text('Chat · General'), findsOneWidget);
    expect(find.byType(MemberPanel), findsNothing);

    // Open right pane (people) via ToggleMembersIntent
    Actions.invoke(
      tester.element(find.byKey(ShellKeys.main)),
      const ToggleMembersIntent(),
    );
    await tester.pumpAndSettle();

    // Chat is closed, MemberPanel is open
    expect(find.text('Chat · General'), findsNothing);
    expect(find.byType(MemberPanel), findsOneWidget);

    // Reverse: open chat closes right pane
    await tester.tap(find.byType(VoiceChatButton));
    await tester.pumpAndSettle();

    expect(find.byType(MemberPanel), findsNothing);
    expect(find.text('Chat · General'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('chat_button_toggles_twice', (tester) async {
    // chat_button_toggles_twice, from the dock and, when alone, from the header.
    // 1. From the dock (wide 1600):
    final app = await MockApp.pump(tester, size: const Size(1600, 800));
    await _joinGeneral(tester, app);

    final dockChatBtn = find.descendant(
      of: find.byType(VoiceControlBar),
      matching: find.byType(VoiceChatButton),
    );

    // First toggle: open
    await tester.tap(dockChatBtn);
    await tester.pumpAndSettle();
    expect(app.read(voiceViewPrefsProvider).chatOpen, isTrue);

    // First toggle: close
    await tester.tap(dockChatBtn);
    await tester.pumpAndSettle();
    expect(app.read(voiceViewPrefsProvider).chatOpen, isFalse);

    // Second toggle: open
    await tester.tap(dockChatBtn);
    await tester.pumpAndSettle();
    expect(app.read(voiceViewPrefsProvider).chatOpen, isTrue);

    // Second toggle: close
    await tester.tap(dockChatBtn);
    await tester.pumpAndSettle();
    expect(app.read(voiceViewPrefsProvider).chatOpen, isFalse);

    await app.dispose(tester);

    // 2. When alone (narrow 1280): from the header
    final appNarrow = await MockApp.pump(tester, size: const Size(1280, 800));
    await _joinGeneral(tester, appNarrow);

    final dockChatBtnNarrow = find.descendant(
      of: find.byType(VoiceControlBar),
      matching: find.byType(VoiceChatButton),
    );

    // Open from dock
    await tester.tap(dockChatBtnNarrow);
    await tester.pumpAndSettle();
    expect(appNarrow.read(voiceViewPrefsProvider).chatOpen, isTrue);

    final headerChatBtn = find.descendant(
      of: find.byType(ChatHeader),
      matching: find.byType(VoiceChatButton),
    );

    // First toggle from header: close
    await tester.tap(headerChatBtn);
    await tester.pumpAndSettle();
    expect(appNarrow.read(voiceViewPrefsProvider).chatOpen, isFalse);

    // Reopen from dock
    await tester.tap(find.descendant(
      of: find.byType(VoiceControlBar),
      matching: find.byType(VoiceChatButton),
    ));
    await tester.pumpAndSettle();
    expect(appNarrow.read(voiceViewPrefsProvider).chatOpen, isTrue);

    // Second toggle from header: close
    await tester.tap(find.descendant(
      of: find.byType(ChatHeader),
      matching: find.byType(VoiceChatButton),
    ));
    await tester.pumpAndSettle();
    expect(appNarrow.read(voiceViewPrefsProvider).chatOpen, isFalse);

    await appNarrow.dispose(tester);
  });

  testWidgets('join_prompt_numbers', (tester) async {
    // Glyph 64, radius 20, selected, volumeUp at 20.
    // Name 17/600; 4 connected 12.5 textSecondary; the faces as today;
    // Join voice: height 36, padding 0 18, radius 10.
    final app = await MockApp.pump(tester);
    final voice = _voiceChannel(app);
    app.read(navigationProvider.notifier).openChannel(_dev, voice);
    await tester.pumpAndSettle();

    final colors = tester.element(find.byType(VoiceView)).oc;

    // Glyph 64, radius 20, selected, volumeUp at 20
    final glyphFinder = find.byWidgetPredicate(
      (widget) =>
          widget is Container &&
          widget.constraints?.minWidth == 64 &&
          widget.constraints?.minHeight == 64,
    );
    expect(glyphFinder, findsOneWidget);
    final glyph = tester.widget<Container>(glyphFinder);
    final deco = glyph.decoration as BoxDecoration;
    expect(deco.color, colors.selected);
    expect(deco.borderRadius, BorderRadius.circular(20));

    final icon = tester.widget<Icon>(
      find.descendant(of: glyphFinder, matching: find.byIcon(OcIcons.volumeUp)),
    );
    expect(icon.size, 20);
    expect(icon.color, colors.text);

    // Name 17/600
    final nameText = tester.widget<Text>(
      find.descendant(
        of: find.byType(VoiceView),
        matching: find.text('General'),
      ),
    );
    expect(nameText.style?.fontSize, 17);
    expect(nameText.style?.fontWeight, FontWeight.w600);

    // 4 connected 12.5 textSecondary
    final connectedText = tester.widget<Text>(
      find.descendant(
        of: find.byType(VoiceView),
        matching: find.text('4 connected'),
      ),
    );
    expect(connectedText.style?.fontSize, 12.5);
    expect(connectedText.style?.color, colors.textSecondary);

    // Join voice: height 36, padding 0 18, radius 10
    final joinBtn = find.widgetWithText(OcButton, 'Join voice');
    expect(joinBtn, findsOneWidget);
    final btnWidget = tester.widget<OcButton>(joinBtn);
    expect(btnWidget.height, 36);
    expect(btnWidget.padding, const EdgeInsets.symmetric(horizontal: 18));
    expect(btnWidget.dense, isFalse);
    final box = tester.widget<Container>(
      find.descendant(of: joinBtn, matching: find.byType(Container)).first,
    );
    expect((box.decoration as BoxDecoration).borderRadius, BorderRadius.circular(10));

    await app.dispose(tester);
  });

  testWidgets('join_prompt_rises_on_the_curve', (tester) async {
    // rises 6 on pop, with a fade.
    final app = await MockApp.pump(tester);
    final voice = _voiceChannel(app);
    app.read(navigationProvider.notifier).openChannel(_dev, voice);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 20));

    // Mid-motion: offset has started rising from translateY(6 -> 0), opacity fading in
    await tester.pump(const Duration(milliseconds: 40));
    final motionFinder = find.byKey(const ValueKey('join-prompt-motion'));
    final transform = tester.widget<Transform>(motionFinder);
    expect(transform.transform.getTranslation().y, lessThan(6.0));
    expect(transform.transform.getTranslation().y, greaterThan(0.0));

    final opacity = tester.widget<Opacity>(
      find.descendant(of: motionFinder, matching: find.byType(Opacity)).first,
    );
    expect(opacity.opacity, greaterThan(0.0));
    expect(opacity.opacity, lessThanOrEqualTo(1.0));

    await tester.pumpAndSettle();
    final transformSettled = tester.widget<Transform>(motionFinder);
    expect(transformSettled.transform.getTranslation().y, 0.0);

    final opacitySettled = tester.widget<Opacity>(
      find.descendant(of: motionFinder, matching: find.byType(Opacity)).first,
    );
    expect(opacitySettled.opacity, 1.0);

    await app.dispose(tester);
  });

  testWidgets('no_permission_note_stays', (tester) async {
    final app = await MockApp.pump(tester);
    final voice = _voiceChannel(app);

    // Deny connect permission
    final serverData = app.read(serverProvider(_dev)).data!;
    final updatedPermissions =
        (serverData.channelPermissions[voice] ?? Permissions.all) -
        Permissions.connect;
    app.read(serverProvider(_dev).notifier).apply(
          PermissionsChanged(_dev, serverData.serverPermissions, {
            ...serverData.channelPermissions,
            voice: updatedPermissions,
          }),
        );

    app.read(navigationProvider.notifier).openChannel(_dev, voice);
    await tester.pumpAndSettle();

    expect(
      find.text("You don't have permission to join this channel."),
      findsOneWidget,
    );
    final btn = tester.widget<OcButton>(
      find.widgetWithText(OcButton, 'Join voice'),
    );
    expect(btn.onPressed, isNull);

    await app.dispose(tester);
  });

  testWidgets('faces_stay', (tester) async {
    final app = await MockApp.pump(tester);
    final voice = _voiceChannel(app);
    app.read(navigationProvider.notifier).openChannel(_dev, voice);
    await tester.pumpAndSettle();

    // Overlapping avatar row has 4 participants
    final avatars = find.descendant(
      of: find.byType(VoiceView),
      matching: find.byType(MemberAvatar),
    );
    expect(avatars, findsNWidgets(4));
    final align = tester.widget<Align>(
      find.ancestor(of: avatars.first, matching: find.byType(Align)).first,
    );
    expect(align.widthFactor, 0.8);

    await app.dispose(tester);
  });
}
