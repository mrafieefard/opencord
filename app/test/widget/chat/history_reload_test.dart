import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

int _channel(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Finder _bubble(int id) => find.byWidgetPredicate(
  (widget) => widget is MessageBubble && widget.message.id == id,
);

void main() {
  for (final delay in [Duration.zero, const Duration(milliseconds: 250)]) {
    testWidgets('a new session leaves the open channel where the reader was '
        '(history in ${delay.inMilliseconds} ms)', (tester) async {
      final app = await MockApp.pump(tester);
      app
          .read(navigationProvider.notifier)
          .openChannel(_dev, _channel(app, 'general'));
      await tester.pump(const Duration(milliseconds: 800));
      await tester.drag(find.byType(MessageList), const Offset(0, 400));
      await tester.pumpAndSettle();
      final reading = tester
          .widget<MessageBubble>(find.byType(MessageBubble).first)
          .message
          .id;
      final where = tester.getTopLeft(_bubble(reading));

      app.repository
        ..debugHistoryDelay = delay
        ..debugDisconnect(_dev)
        ..debugReconnect(_dev);
      // Frames just after the new Ready, while the newest page reloads.
      for (var i = 0; i < 8; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      // Pages still on their way arrive.
      await tester.pump(const Duration(seconds: 1));
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expect(tester.getTopLeft(_bubble(reading)), where);
      await app.dispose(tester);
    });
  }

  testWidgets('a channel that could not load says so, and tries again', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app.repository.debugFailHistory = true;
    app
        .read(navigationProvider.notifier)
        .openChannel(_dev, _channel(app, 'dev-core'));
    await tester.pump(const Duration(milliseconds: 800));
    // The failure comes in after the frame that opened the channel.
    await tester.pump();

    expect(find.text("Couldn't load messages"), findsOneWidget);
    expect(find.byType(MessageBubble), findsNothing);

    app.repository.debugFailHistory = false;
    await tester.tap(find.widgetWithText(OcButton, 'Try again'));
    await tester.pumpAndSettle();

    expect(find.text("Couldn't load messages"), findsNothing);
    expect(find.byType(MessageBubble), findsWidgets);
    await app.dispose(tester);
  });
}
