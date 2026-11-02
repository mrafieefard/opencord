// A private chat scrolls as a channel does (§6): it opens at the newest
// message, or where the reader left it, and follows new messages while
// the reader is at the newest one.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/home/private_message_list.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

import '../../support/app.dart';

String _kai(MockApp app) => app.repository.privateWorld.contacts.values
    .firstWhere((c) => c.name == 'Kai Nakamura')
    .peer;

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 10; i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

Future<void> _pumpChat(WidgetTester tester, MockApp app, String peer) async {
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(body: PrivateChat(peer: peer)),
      ),
    ),
  );
  await _settle(tester);
}

/// Whether [text] is drawn inside the list's visible area; a text ending
/// in a space is matched without a digit after it, so `Old message 1 `
/// isn't `Old message 10`.
bool _inView(WidgetTester tester, String text) {
  final found = find.textContaining(
    text.endsWith(' ') ? RegExp('${RegExp.escape(text.trim())}(?!\\d)') : text,
    findRichText: true,
  );
  if (found.evaluate().isEmpty) return false;
  final rect = tester.getRect(found.first);
  final list = tester.getRect(find.byType(PrivateMessageList));
  return rect.top >= list.top - 1 && rect.bottom <= list.bottom + 1;
}

/// The first `Old message n` wholly in view.
String _firstInView(WidgetTester tester) {
  final list = tester.getRect(find.byType(PrivateMessageList));
  final shown = <(double, String)>[];
  for (final element
      in find.textContaining('Old message ', findRichText: true).evaluate()) {
    final widget = element.widget;
    final text = switch (widget) {
      RichText(:final text) => text.toPlainText(),
      Text(:final data?) => data,
      _ => null,
    };
    final box = element.renderObject;
    if (text == null || box is! RenderBox || !box.hasSize) continue;
    final rect = box.localToGlobal(Offset.zero) & box.size;
    if (rect.top >= list.top && rect.bottom <= list.bottom) {
      shown.add((rect.top, text.trim()));
    }
  }
  shown.sort((a, b) => a.$1.compareTo(b.$1));
  return shown.first.$2;
}

Future<void> _scrollUp(WidgetTester tester, double by) async {
  await tester.drag(
    find.byType(PrivateMessageList),
    Offset(0, by),
    warnIfMissed: false,
  );
  await _settle(tester);
}

void main() {
  testWidgets('a_long_chat_opens_at_the_newest_message', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);

    await _pumpChat(tester, app, kai);

    expect(_inView(tester, 'Sure, I will move over in a bit.'), isTrue);
    expect(_inView(tester, 'Old message 1 '), isFalse);
    await app.dispose(tester);
  });

  testWidgets('new_messages_are_followed_at_the_newest', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);
    await _pumpChat(tester, app, kai);

    for (var i = 1; i <= 3; i++) {
      app.repository.privateWorld.debugReceive(kai, 'Fresh news $i');
      await _settle(tester);
    }

    expect(_inView(tester, 'Fresh news 3'), isTrue);
    await app.dispose(tester);
  });

  testWidgets('reading_older_is_not_moved_by_new_ones', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);
    await _pumpChat(tester, app, kai);
    await _scrollUp(tester, 600);
    final before = _firstInView(tester);

    app.repository.privateWorld.debugReceive(kai, 'Fresh news');
    await _settle(tester);

    expect(_firstInView(tester), before);
    expect(_inView(tester, 'Fresh news'), isFalse);
    await app.dispose(tester);
  });

  testWidgets('sending_goes_to_the_newest_message', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);
    await _pumpChat(tester, app, kai);
    await _scrollUp(tester, 900);

    await tester.enterText(find.byType(TextField), 'My own reply');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settle(tester);
    await tester.pump(const Duration(seconds: 1));
    await _settle(tester);

    expect(_inView(tester, 'My own reply'), isTrue);
    await app.dispose(tester);
  });

  testWidgets('older_messages_load_near_the_top', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);
    await _pumpChat(tester, app, kai);

    for (var i = 0; i < 40 && !_inView(tester, 'Old message 1 '); i++) {
      await _scrollUp(tester, 2000);
    }

    expect(_inView(tester, 'Old message 1 '), isTrue);
    await app.dispose(tester);
  });

  testWidgets('coming_back_returns_to_where_the_reader_was', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);
    await _pumpChat(tester, app, kai);
    await _scrollUp(tester, 900);
    final left = _firstInView(tester);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: app.container,
        child: const SizedBox(),
      ),
    );
    await _pumpChat(tester, app, kai);

    expect(_firstInView(tester), left);
    await app.dispose(tester);
  });

  testWidgets('a_restart_returns_there_even_past_the_newest_page', (
    tester,
  ) async {
    final store = MemoryKeyValueStore();
    var app = await MockApp.pump(tester, store: store);
    final kai = _kai(app);
    app.repository.privateWorld.debugLongHistory(kai, 120);
    await _pumpChat(tester, app, kai);
    for (var i = 0; i < 40 && !_inView(tester, 'Old message 20 '); i++) {
      await _scrollUp(tester, 1500);
    }
    final left = _firstInView(tester);
    await tester.pump(const Duration(seconds: 2));
    app.read(scrollMemoryProvider).flush();
    await app.dispose(tester);

    app = await MockApp.pump(tester, store: store);
    app.repository.privateWorld.debugLongHistory(_kai(app), 120);
    await _pumpChat(tester, app, _kai(app));

    expect(_firstInView(tester), left);
    await app.dispose(tester);
  });
}
