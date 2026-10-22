import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/activity_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';

/// Whether a new message notifies, and as what.
enum NotifyAs { none, message, mention }

/// What a new message does on the desktop (§8.1 Notifications, §15):
/// nothing while the window is focused, in do not disturb or with
/// notifications off; in muted channels and with "mentions only", just
/// mentions of the user.
NotifyAs notifyAs({
  required Message message,
  required int selfId,
  required AppSettings settings,
  required bool muted,
  required bool windowFocused,
  required SelfPresence presence,
}) {
  if (!settings.desktopNotifications ||
      windowFocused ||
      presence == SelfPresence.doNotDisturb ||
      message.authorId == selfId ||
      message.isSystem) {
    return NotifyAs.none;
  }
  if (mentionsUser(message.content, selfId)) return NotifyAs.mention;
  if (muted || settings.mentionsOnly) return NotifyAs.none;
  return NotifyAs.message;
}

/// Whether a new message asks for the user's attention (§15): the window
/// urgency hint on Linux, a taskbar flash on Windows, a Dock bounce on
/// macOS. Only mentions, while the window is away, with "flash taskbar"
/// on and not in do not disturb.
bool wantsAttention({
  required Message message,
  required int selfId,
  required AppSettings settings,
  required bool windowFocused,
  required SelfPresence presence,
}) =>
    settings.flashTaskbar &&
    !windowFocused &&
    presence != SelfPresence.doNotDisturb &&
    message.authorId != selfId &&
    !message.isSystem &&
    mentionsUser(message.content, selfId);
