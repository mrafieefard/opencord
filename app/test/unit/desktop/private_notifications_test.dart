import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/desktop/notification_binding.dart';
import 'package:opencord/features/desktop/notification_policy.dart';
import 'package:opencord/features/desktop/notifications.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/window_title.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_providers.dart';

import '../../support/fake_notifications.dart';
import '../../support/fake_window.dart';

final _peerAlice = 'a' * 64;
final _peerBob = 'b' * 64;

PrivateContact _contact({
  required String peer,
  String name = 'Alice',
  ConversationStatus conversation = ConversationStatus.accepted,
  bool muted = false,
  bool blocked = false,
  int unread = 0,
}) => PrivateContact(
  peer: peer,
  fingerprint: 'AAAA-BBBB-CCCC-DDDD',
  name: name,
  friend: FriendStatus.friends,
  conversation: conversation,
  muted: muted,
  blocked: blocked,
  unread: unread,
);

PrivateMessage _message({
  required String peer,
  String content = 'Hello there',
  bool outgoing = false,
  PrivateMessageKind kind = PrivateMessageKind.text,
}) => PrivateMessage(
  id: 'msg-1',
  peer: peer,
  outgoing: outgoing,
  kind: kind,
  content: content,
  sentAt: DateTime(2026, 10, 24, 10, 0),
  state: PrivateSendState.received,
);

void main() {
  group('private notifications (§8.1, §15)', () {
    late FakeNotifications notifications;
    late FakeNativeWindow window;
    late MockRepository mockRepo;
    late ProviderContainer container;

    setUp(() {
      final store = MemoryKeyValueStore();
      notifications = FakeNotifications();
      window = FakeNativeWindow();
      mockRepo = MockRepository(simulateLife: false);
      container = ProviderContainer(
        overrides: [
          keyValueStoreProvider.overrideWithValue(store),
          notificationServiceProvider.overrideWithValue(notifications),
          nativeWindowProvider.overrideWithValue(window),
          repositoryProvider.overrideWithValue(mockRepo),
        ],
      );
      container.read(eventPumpProvider);
      container.read(notificationBindingProvider);
    });

    tearDown(() {
      container.dispose();
    });

    test(
      'private_messages_notify_only_in_accepted_unmuted_conversations',
      () async {
        final alice = _contact(peer: _peerAlice, name: 'Alice');
        final bob = _contact(
          peer: _peerBob,
          name: 'Bob',
          conversation: ConversationStatus.request,
        );

        // Populate contacts in state
        mockRepo.emit(PrivateContactChanged(alice));
        mockRepo.emit(PrivateContactChanged(bob));
        await pumpEventQueue();

        // 1. Incoming text message in accepted conversation notifies
        mockRepo.emit(
          PrivateMessageAdded(
            _message(peer: _peerAlice, content: 'Hey Alice here'),
          ),
        );
        await pumpEventQueue();
        expect(notifications.shown, hasLength(1));
        final shown = notifications.shown.last;
        expect(shown.title, 'Alice');
        expect(shown.body, 'Hey Alice here');
        expect(shown.link, 'opencord://private/$_peerAlice');
        expect(shown.sound, isTrue);

        // 2. Incoming text message in request conversation does NOT notify
        notifications.shown.clear();
        mockRepo.emit(
          PrivateMessageAdded(
            _message(peer: _peerBob, content: 'Hey Bob request'),
          ),
        );
        await pumpEventQueue();
        expect(notifications.shown, isEmpty);

        // 3. Muted conversation does NOT notify
        final mutedAlice = alice.copyWith(muted: true);
        mockRepo.emit(PrivateContactChanged(mutedAlice));
        await pumpEventQueue();
        mockRepo.emit(
          PrivateMessageAdded(_message(peer: _peerAlice, content: 'Muted msg')),
        );
        await pumpEventQueue();
        expect(notifications.shown, isEmpty);

        // 4. Blocked conversation does NOT notify
        final blockedAlice = alice.copyWith(muted: false, blocked: true);
        mockRepo.emit(PrivateContactChanged(blockedAlice));
        await pumpEventQueue();
        mockRepo.emit(
          PrivateMessageAdded(
            _message(peer: _peerAlice, content: 'Blocked msg'),
          ),
        );
        await pumpEventQueue();
        expect(notifications.shown, isEmpty);

        // 5. Outgoing message does NOT notify
        mockRepo.emit(PrivateContactChanged(alice));
        await pumpEventQueue();
        mockRepo.emit(
          PrivateMessageAdded(_message(peer: _peerAlice, outgoing: true)),
        );
        await pumpEventQueue();
        expect(notifications.shown, isEmpty);

        // 6. Non-text message does NOT notify
        mockRepo.emit(
          PrivateMessageAdded(
            _message(peer: _peerAlice, kind: PrivateMessageKind.divider),
          ),
        );
        await pumpEventQueue();
        expect(notifications.shown, isEmpty);
      },
    );

    test('do_not_disturb_and_an_open_chat_silence_them', () async {
      final alice = _contact(peer: _peerAlice, name: 'Alice');
      mockRepo.emit(PrivateContactChanged(alice));
      await pumpEventQueue();

      // Policy checks
      final settings = container.read(appSettingsProvider);
      expect(
        notifyPrivate(
          settings: settings,
          windowFocused: true,
          chatOpen: true,
          presence: SelfPresence.online,
        ),
        NotifyAs.none,
      );
      expect(
        notifyPrivate(
          settings: settings,
          windowFocused: false,
          chatOpen: true,
          presence: SelfPresence.online,
        ),
        NotifyAs.message,
      );
      expect(
        notifyPrivate(
          settings: settings,
          windowFocused: true,
          chatOpen: false,
          presence: SelfPresence.online,
        ),
        NotifyAs.message,
      );
      expect(
        notifyPrivate(
          settings: settings,
          windowFocused: false,
          chatOpen: false,
          presence: SelfPresence.doNotDisturb,
        ),
        NotifyAs.none,
      );
      expect(
        notifyPrivate(
          settings: settings.copyWith(desktopNotifications: false),
          windowFocused: false,
          chatOpen: false,
          presence: SelfPresence.online,
        ),
        NotifyAs.none,
      );

      // End-to-end: Open chat while focused silences notification
      container.read(navigationProvider.notifier).openConversation(_peerAlice);
      window.statusEvents.add(const WindowStatus(focused: true));
      await pumpEventQueue();

      mockRepo.emit(
        PrivateMessageAdded(
          _message(peer: _peerAlice, content: 'While chat is open'),
        ),
      );
      await pumpEventQueue();
      expect(notifications.shown, isEmpty);

      // End-to-end: DND silences notification even when window unfocused
      container.read(navigationProvider.notifier).openFriends();
      window.statusEvents.add(const WindowStatus(focused: false));
      container
          .read(selfPresenceProvider.notifier)
          .choose(SelfPresence.doNotDisturb);
      await pumpEventQueue();

      mockRepo.emit(
        PrivateMessageAdded(_message(peer: _peerAlice, content: 'While DND')),
      );
      await pumpEventQueue();
      expect(notifications.shown, isEmpty);
    });

    test('friend_requests_notify_unless_spam', () async {
      final alice = _contact(peer: _peerAlice, name: 'Alice');
      mockRepo.emit(PrivateContactChanged(alice));
      await pumpEventQueue();

      // Spam friend request does not notify
      mockRepo.emit(FriendRequestArrived(_peerAlice, true));
      await pumpEventQueue();
      expect(notifications.shown, isEmpty);

      // Non-spam friend request notifies
      mockRepo.emit(FriendRequestArrived(_peerAlice, false));
      await pumpEventQueue();
      expect(notifications.shown, hasLength(1));
      final shown = notifications.shown.last;
      expect(shown.title, 'Alice');
      expect(shown.body, 'Alice sent you a friend request');
      expect(shown.link, 'opencord://friends');
      expect(shown.sound, isTrue);
    });

    test('the_window_title_counts_private_unread', () async {
      final baseUnread = container.read(unreadTotalProvider);
      final prefix = baseUnread > 0 ? '($baseUnread) ' : '';

      // 1. Home shown, friends view, initial unreads
      container.read(navigationProvider.notifier).openFriends();
      expect(
        container.read(windowTitleProvider),
        '${prefix}Friends — Opencord',
      );

      // 2. Open conversation with Alice
      final alice = _contact(peer: _peerAlice, name: 'Alice');
      mockRepo.emit(PrivateContactChanged(alice));
      await pumpEventQueue();
      container.read(navigationProvider.notifier).openConversation(_peerAlice);
      expect(container.read(windowTitleProvider), '${prefix}Alice — Opencord');

      // 3. Unread private messages update window title with prefix count
      final aliceWithUnread = alice.copyWith(unread: 3);
      mockRepo.emit(PrivateContactChanged(aliceWithUnread));
      await pumpEventQueue();

      final newTotal = baseUnread + 3;
      expect(
        container.read(windowTitleProvider),
        '($newTotal) Alice — Opencord',
      );

      container.read(navigationProvider.notifier).openFriends();
      expect(
        container.read(windowTitleProvider),
        '($newTotal) Friends — Opencord',
      );
    });
  });
}
