import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/chat/message_item.dart';
import 'package:opencord/features/chat/suggestion_list.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';
const _priya = 1005;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

int _channelId(MockApp app, String name, {String server = _server}) => app
    .read(serverProvider(server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<ChannelRef> _open(
  WidgetTester tester,
  MockApp app,
  String name, {
  String server = _server,
}) async {
  final id = _channelId(app, name, server: server);
  app.read(navigationProvider.notifier).openChannel(server, id);
  await _pumpFor(tester, const Duration(milliseconds: 300));
  return (server: server, channel: id);
}

Finder get _input => find.descendant(
  of: find.byType(Composer),
  matching: find.byType(TextField),
);

String _text(WidgetTester tester) =>
    tester.widget<TextField>(_input).controller!.text;

Future<void> _type(WidgetTester tester, String text) async {
  await tester.enterText(_input, text);
  await tester.pump();
}

Future<void> _press(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyEvent(key);
  await tester.pump();
}

List<Message> _messages(MockApp app, ChannelRef channel) =>
    app.read(channelMessagesProvider(channel)).all;

Finder _button(String tooltip) => find.byWidgetPredicate(
  (widget) => widget is OcIconButton && widget.tooltip == tooltip,
);

void main() {
  testWidgets('Enter sends; the composer empties and the message shows', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = await _open(tester, app, 'general');
    expect(find.text('Message #general'), findsOneWidget);
    expect(_button('Voice message'), findsOneWidget);

    await _type(tester, 'Hello from the composer');
    expect(_button('Send'), findsOneWidget);
    await _press(tester, LogicalKeyboardKey.enter);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(_text(tester), isEmpty);
    expect(_messages(app, general).last.content, 'Hello from the composer');
    expect(_messages(app, general).last.sendState, SendState.sent);
    await app.dispose(tester);
  });

  testWidgets('a reply shows above the input and is sent as a reply', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = await _open(tester, app, 'general');
    final original = _messages(
      app,
      general,
    ).lastWhere((m) => m.authorId == _priya);
    app.read(composerProvider(general).notifier).reply(original);
    await tester.pump();
    expect(find.text('Reply to Priya Shah'), findsOneWidget);

    await _type(tester, 'Sure');
    await _press(tester, LogicalKeyboardKey.enter);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.text('Reply to Priya Shah'), findsNothing);
    expect(_messages(app, general).last.replyToId, original.id);
    await app.dispose(tester);
  });

  testWidgets('Escape cancels the reply and keeps the text', (tester) async {
    final app = await MockApp.pump(tester);
    final general = await _open(tester, app, 'general');
    app
        .read(composerProvider(general).notifier)
        .reply(_messages(app, general).first);
    await _type(tester, 'half');

    await _press(tester, LogicalKeyboardKey.escape);

    expect(app.read(composerProvider(general)).replyTo, isNull);
    expect(_text(tester), 'half');
    await app.dispose(tester);
  });

  testWidgets('↑ in an empty composer edits your last message', (tester) async {
    final app = await MockApp.pump(tester);
    final general = await _open(tester, app, 'general');
    final mine = _messages(app, general).lastWhere((m) => m.authorId == 1000);
    await tester.tap(_input);
    await tester.pump();

    await _press(tester, LogicalKeyboardKey.arrowUp);
    expect(find.text('Edit message'), findsOneWidget);
    expect(_text(tester), isNotEmpty);
    expect(_text(tester), isNot(contains('<@')));

    await _type(tester, 'Edited by hand');
    await _press(tester, LogicalKeyboardKey.enter);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final edited = _messages(app, general).firstWhere((m) => m.id == mine.id);
    expect(edited.content, 'Edited by hand');
    expect(edited.edited, isTrue);
    expect(_text(tester), isEmpty);
    await app.dispose(tester);
  });

  testWidgets('@ suggests members and sends the pick as a mention', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = await _open(tester, app, 'general');

    await _type(tester, 'hey @Pri');
    expect(find.byType(SuggestionList), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(SuggestionList),
        matching: find.text('Priya Shah'),
      ),
      findsOneWidget,
    );
    await _press(tester, LogicalKeyboardKey.enter);
    expect(_text(tester), 'hey @Priya Shah ');
    expect(find.byType(SuggestionList), findsNothing);

    await _type(tester, '${_text(tester)}thanks');
    await _press(tester, LogicalKeyboardKey.enter);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(_messages(app, general).last.content, 'hey <@$_priya> thanks');
    await app.dispose(tester);
  });

  testWidgets(': suggests emoji; Tab picks; Escape closes', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await _type(tester, 'party :tada');
    await _press(tester, LogicalKeyboardKey.tab);
    expect(_text(tester), 'party 🎉 ');

    await _type(tester, 'more :fir');
    expect(find.byType(SuggestionList), findsOneWidget);
    await _press(tester, LogicalKeyboardKey.escape);
    expect(find.byType(SuggestionList), findsNothing);
    expect(_text(tester), 'more :fir');
    await app.dispose(tester);
  });

  testWidgets('each channel keeps its own draft', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _type(tester, 'unfinished thought');

    await _open(tester, app, 'help');
    expect(_text(tester), isEmpty);
    await _open(tester, app, 'general');

    expect(_text(tester), 'unfinished thought');
    await app.dispose(tester);
  });

  testWidgets('without permission the composer says so', (tester) async {
    final app = await MockApp.pump(tester);
    const berlin = 'rust-berlin.example:7710';
    app.read(navigationProvider.notifier).openServer(berlin);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await _open(tester, app, 'announcements', server: berlin);

    expect(
      find.text('You do not have permission to send messages in this channel.'),
      findsOneWidget,
    );
    expect(_input, findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a rate limit shows a countdown until sending works again', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    app.repository.debugRateLimit(const Duration(seconds: 5));

    await _type(tester, 'too fast');
    await _press(tester, LogicalKeyboardKey.enter);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.textContaining(RegExp(r'^\d+ s$')), findsOneWidget);
    await _pumpFor(tester, const Duration(seconds: 5));
    expect(find.textContaining(RegExp(r'^\d+ s$')), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('typing tells others at most every eight seconds', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    final before = app.repository.typingSignals;

    await _type(tester, 'h');
    await _type(tester, 'he');
    await _type(tester, 'hey');
    expect(app.repository.typingSignals, before + 1);

    await _pumpFor(tester, const Duration(seconds: 9));
    await _type(tester, 'hey!');
    expect(app.repository.typingSignals, before + 2);
    await app.dispose(tester);
  });

  testWidgets('typing anywhere goes to the composer', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    Focus.of(
      tester.element(
        find
            .descendant(
              of: find.byType(MessageItem).first,
              matching: find.byType(Listener),
            )
            .first,
      ),
    ).requestFocus();
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.keyH);
    await tester.pump();

    expect(_text(tester), 'h');
    final field = tester.widget<TextField>(_input);
    expect(field.focusNode!.hasFocus, isTrue);
    await app.dispose(tester);
  });
}
