import 'dart:convert';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/user_audio_prefs.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/voice/user_audio_controls.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _mira = 1002;
const _jonas = 1003;

/// Tall enough for every voice row to show above the voice panel.
const _size = Size(1440, 1300);

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder _inMenu(Finder finder) =>
    find.descendant(of: find.byType(OcMenuPanel), matching: finder);

Future<void> _joinGeneral(WidgetTester tester) async {
  await tester.tap(_inSidebar('General'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _openMenu(WidgetTester tester, Finder target) async {
  await tester.tap(target, buttons: kSecondaryButton);
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _closeMenu(WidgetTester tester) async {
  await tester.sendKeyEvent(LogicalKeyboardKey.escape);
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

void main() {
  testWidgets(
    "someone's volume, Mute and Mute soundboard work here and are kept",
    (tester) async {
      final app = await MockApp.pump(tester, size: _size);
      await _joinGeneral(tester);
      await _openMenu(tester, _inSidebar('Mira Okafor'));

      expect(_inMenu(find.text('User volume')), findsOneWidget);
      expect(_inMenu(find.text('100%')), findsOneWidget);
      await tester.tap(_inMenu(find.text('Mute')));
      await tester.pump();
      // The toggles leave the menu open, like Discord's.
      expect(find.byType(OcMenuPanel), findsOneWidget);
      tester.widget<Slider>(_inMenu(find.byType(Slider))).onChanged!(150);
      await tester.pump();
      expect(_inMenu(find.text('150%')), findsOneWidget);
      await tester.tap(_inMenu(find.text('Mute soundboard')));
      await tester.pump();

      expect(app.repository.listening[(_dev, _mira)], (150, true));
      expect(app.repository.mutedSoundboards, contains((_dev, _mira)));
      await _closeMenu(tester);
      // Muted here shows on their row, for this device only.
      expect(
        find.descendant(
          of: find.byKey(DesktopShell.sidebarKey),
          matching: find.byIcon(OcIcons.volumeOff),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byKey(DesktopShell.sidebarKey),
          matching: find.bySemanticsLabel(RegExp('^Mira Okafor, muted by you')),
        ),
        findsOneWidget,
      );
      final saved = jsonDecode(app.store.read(userAudioPrefsKey)!) as Map;
      expect(saved.values.single, {
        'volume': 150,
        'muted': true,
        'soundboardMuted': true,
      });
      final store = app.store;
      await app.dispose(tester);

      // After a restart, joining hands the choices to voice media again.
      final again = await MockApp.pump(tester, size: _size, store: store);
      expect(again.repository.listening, isEmpty);
      await _joinGeneral(tester);
      expect(again.repository.listening[(_dev, _mira)], (150, true));
      expect(again.repository.mutedSoundboards, contains((_dev, _mira)));
      await again.dispose(tester);
    },
  );

  testWidgets('a choice reaches someone when they join your channel', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: _size);
    final store = app.read(userAudioPrefsProvider.notifier);
    final key = app.read(userAudioKeyProvider((server: _dev, user: _mira)))!;
    store.update(key, (audio) => audio.copyWith(volume: 40));
    expect(app.repository.listening, isEmpty, reason: 'not in voice yet');

    await _joinGeneral(tester);
    expect(app.repository.listening[(_dev, _mira)], (40, false));

    // Back to the default: voice media hears that too.
    store.update(key, (_) => const UserAudio());
    await tester.pump();
    expect(app.repository.listening[(_dev, _mira)], (100, false));
    expect(app.read(userAudioPrefsProvider), isEmpty);
    await app.dispose(tester);
  });

  testWidgets('the voice part is only for others in voice', (tester) async {
    final app = await MockApp.pump(tester, size: _size);
    await _joinGeneral(tester);

    // Yourself in voice: no volume for yourself.
    await _openMenu(
      tester,
      // The first is the voice row; the user panel's is at the bottom.
      _inSidebar(
        app.read(serverProvider(_dev)).data!.selfMember!.displayName,
      ).first,
    );
    expect(find.byType(OcMenuPanel), findsOneWidget);
    expect(_inMenu(find.byType(UserAudioControls)), findsNothing);
    await _closeMenu(tester);

    // Someone not in voice, from the member list.
    await _openMenu(
      tester,
      find.descendant(
        of: find.byType(MemberRow),
        matching: find.text('Kai Nakamura'),
      ),
    );
    expect(find.byType(OcMenuPanel), findsOneWidget);
    expect(_inMenu(find.byType(UserAudioControls)), findsNothing);
    expect(_inMenu(find.text('Server mute')), findsNothing);
    await _closeMenu(tester);

    // Someone in voice, from the member list.
    await _openMenu(
      tester,
      find.descendant(
        of: find.byType(MemberRow),
        matching: find.text('Mira Okafor'),
      ),
    );
    expect(_inMenu(find.byType(UserAudioControls)), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a moderator can server mute, deafen, move and disconnect', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: _size);
    await _joinGeneral(tester);
    List<int> inChannel(String name) {
      final data = app.read(serverProvider(_dev)).data!;
      final channel = data.channels.values.firstWhere((c) => c.name == name);
      return [
        for (final participant
            in app.read(voiceProvider(_dev))[channel.id] ?? const [])
          participant.userId,
      ];
    }

    await _openMenu(tester, _inSidebar('Jonas Weber'));
    await tester.tap(_inMenu(find.text('Server mute')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.bySemanticsLabel(RegExp('^Jonas Weber, .*server muted')),
      ),
      findsOneWidget,
    );

    await _openMenu(tester, _inSidebar('Jonas Weber'));
    final serverMute = tester.widget<OcMenuPanel>(find.byType(OcMenuPanel));
    expect(
      serverMute.entries
          .whereType<OcMenuItem>()
          .firstWhere((item) => item.label == 'Server mute')
          .checked,
      isTrue,
    );
    await tester.tap(_inMenu(find.text('Server deafen')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.bySemanticsLabel(
          RegExp('^Jonas Weber, .*server deafened'),
        ),
      ),
      findsOneWidget,
    );

    await _openMenu(tester, _inSidebar('Jonas Weber'));
    await tester.tap(_inMenu(find.text('Move to')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(_inMenu(find.text('Pairing')).last);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(inChannel('Pairing'), [_jonas]);
    expect(inChannel('General'), isNot(contains(_jonas)));

    await _openMenu(tester, _inSidebar('Jonas Weber'));
    await tester.tap(_inMenu(find.text('Disconnect')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(inChannel('Pairing'), isEmpty);
    expect(_inSidebar('Jonas Weber'), findsNothing);
    await app.dispose(tester);
  });
}
