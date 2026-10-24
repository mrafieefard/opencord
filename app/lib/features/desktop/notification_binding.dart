import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/attachments/summary.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/features/desktop/notification_policy.dart';
import 'package:opencord/features/desktop/notifications.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/window_providers.dart';

/// How much of a message a notification shows.
const _previewLength = 200;

/// Notifies about new messages while the app is away, asks for attention
/// on mentions until the window is focused again, and opens the message a
/// notification is clicked for (§8.1 Notifications, §15).
final notificationBindingProvider = Provider<void>((ref) {
  final service = ref.watch(notificationServiceProvider);
  final window = ref.watch(nativeWindowProvider);
  var urgent = false;

  void onMessage(String server, Message message) {
    final data = ref.read(serverProvider(server)).data;
    if (data == null) return;
    final settings = ref.read(appSettingsProvider);
    final focused = ref.read(windowStatusProvider).focused;
    final presence = ref.read(selfPresenceProvider);
    if (!urgent &&
        wantsAttention(
          message: message,
          selfId: data.self.id,
          settings: settings,
          windowFocused: focused,
          presence: presence,
        )) {
      urgent = true;
      unawaited(window.setUrgent(true));
    }
    final kind = notifyAs(
      message: message,
      selfId: data.self.id,
      settings: settings,
      muted: ref
          .read(notificationPrefsProvider)
          .muted(server, message.channelId),
      windowFocused: focused,
      presence: presence,
    );
    if (kind == NotifyAs.none) return;
    final author = data.members[message.authorId]?.displayName ?? 'Someone';
    final channel = data.channels[message.channelId];
    final where = channel == null
        ? data.info.name
        : '${channel.kind.isTextLike ? '#' : ''}${channel.name}, '
              '${data.info.name}';
    final text = copyableText(
      messageSummary(message),
      user: (id) => data.members[id]?.displayName,
      channel: (id) => data.channels[id]?.name,
    );
    unawaited(
      service.show(
        DesktopNotification(
          title: '$author ($where)',
          body: text.length > _previewLength
              ? '${text.substring(0, _previewLength)}…'
              : text,
          link: messageLink(server, message.channelId, message.id),
          sound: settings.messageSound,
        ),
      ),
    );
  }

  void onPrivateMessage(PrivateMessage message) {
    if (message.outgoing || message.kind != PrivateMessageKind.text) return;
    final contact = ref.read(privateContactsProvider).byPeer[message.peer];
    if (contact == null ||
        contact.conversation != ConversationStatus.accepted ||
        contact.muted ||
        contact.blocked) {
      return;
    }
    final settings = ref.read(appSettingsProvider);
    final focused = ref.read(windowStatusProvider).focused;
    final presence = ref.read(selfPresenceProvider);
    if (!urgent &&
        wantsPrivateAttention(
          settings: settings,
          windowFocused: focused,
          presence: presence,
        )) {
      urgent = true;
      unawaited(window.setUrgent(true));
    }
    final homeShown = ref.read(homeShownProvider);
    final currentPeer = ref.read(currentPeerProvider);
    final chatOpen = homeShown && currentPeer == message.peer;
    if (notifyPrivate(
          settings: settings,
          windowFocused: focused,
          chatOpen: chatOpen,
          presence: presence,
        ) ==
        NotifyAs.none) {
      return;
    }
    final name = ref.read(contactNameProvider(message.peer));
    final text = message.content;
    final body = text.length > _previewLength
        ? '${text.substring(0, _previewLength)}…'
        : text;
    unawaited(
      service.show(
        DesktopNotification(
          title: name,
          body: body,
          link: privateLink(message.peer),
          sound: settings.messageSound,
        ),
      ),
    );
  }

  void onFriendRequest(String peer, bool spam) {
    if (spam) return;
    final settings = ref.read(appSettingsProvider);
    final focused = ref.read(windowStatusProvider).focused;
    final presence = ref.read(selfPresenceProvider);
    if (notifyPrivate(
          settings: settings,
          windowFocused: focused,
          chatOpen: false,
          presence: presence,
        ) ==
        NotifyAs.none) {
      return;
    }
    final name = ref.read(contactNameProvider(peer));
    unawaited(
      service.show(
        DesktopNotification(
          title: name,
          body: '$name sent you a friend request',
          link: friendsLink(),
          sound: settings.messageSound,
        ),
      ),
    );
  }

  final events = ref.watch(repositoryProvider).events.listen((event) {
    switch (event) {
      case MessageCreated(:final serverKey, :final message):
        onMessage(serverKey, message);
      case PrivateMessageAdded(:final message):
        onPrivateMessage(message);
      case FriendRequestArrived(:final peer, :final spam):
        onFriendRequest(peer, spam);
      default:
        break;
    }
  });
  ref.listen(windowStatusProvider.select((status) => status.focused), (
    _,
    focused,
  ) {
    if (focused && urgent) {
      urgent = false;
      unawaited(window.setUrgent(false));
    }
  });
  final clicks = service.opened.listen((link) {
    unawaited(window.show());
    ref.read(appLinkInboxProvider.notifier).add(link);
  });
  ref.onDispose(() {
    events.cancel();
    clicks.cancel();
  });
});

