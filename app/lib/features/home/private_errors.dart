import 'package:flutter/material.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The private action that failed (§6.7).
enum PrivateAction { message, friendRequest, answer, move }

/// Shows a dialog or toast for a failed private messaging action (§6.7).
Future<void> showPrivateFailure(
  BuildContext context,
  RepoException error, {
  required String name,
  String? server,
  PrivateAction action = PrivateAction.message,
}) async {
  String? title;
  String? message;

  switch (error.kind) {
    case RepoErrorKind.friendRequestsClosed:
      title = "Couldn't send friend request";
      message = "$name isn't accepting friend requests.";
      break;

    case RepoErrorKind.messageRequestsClosed:
      title = "Couldn't send message";
      message = "$name only accepts messages from friends.";
      break;

    case RepoErrorKind.undeliverable:
      if (action == PrivateAction.friendRequest) {
        title = "Couldn't send friend request";
        message = "Your friend request couldn't be sent.";
      } else {
        title = "Couldn't send message";
        message = "Your message couldn't be delivered.";
      }
      break;

    case RepoErrorKind.privateMessagesUnavailable:
      if (action == PrivateAction.move) {
        title = "Couldn't move the chat";
        message =
            "Couldn't move the chat: $name can't get private messages through ${server ?? 'this server'}.";
      } else if (action == PrivateAction.friendRequest) {
        title = "Couldn't send friend request";
        message =
            "$name can't get private messages through ${server ?? 'this server'}.";
      } else if (action == PrivateAction.answer) {
        title = "Couldn't answer friend request";
        message =
            "Couldn't answer: ${server ?? 'The server'} isn't connected right now.";
      } else {
        title = "Couldn't send message";
        message =
            "$name can't get private messages through ${server ?? 'this server'}. Try another server you share.";
      }
      break;

    case RepoErrorKind.notConnected:
      if (action == PrivateAction.answer) {
        title = "Couldn't answer friend request";
        message =
            "Couldn't answer: ${server ?? 'The server'} isn't connected right now.";
      }
      break;

    case RepoErrorKind.invalidArgument:
      if (action == PrivateAction.answer &&
          error.message.toLowerCase().contains("couldn't answer")) {
        title = "Couldn't answer friend request";
        message = error.message;
      }
      break;

    case RepoErrorKind.conflict:
      if (action == PrivateAction.friendRequest) {
        title = "Couldn't send friend request";
        final lower = error.message.toLowerCase();
        if (lower.contains('accept') ||
            lower.contains('pending') ||
            lower.contains('incoming') ||
            lower.contains('received')) {
          message =
              "$name already sent you a friend request. Accept it in Friends → Pending.";
        } else {
          message = "You already sent $name a friend request.";
        }
      }
      break;

    default:
      break;
  }

  if (title != null && message != null) {
    await showOcDialog<void>(
      context: context,
      builder: (context) => OcDialog(
        title: title!,
        actions: [
          OcButton.primary(
            label: 'OK',
            autofocus: true,
            onPressed: () => Navigator.pop(context),
          ),
        ],
        child: Text(message!),
      ),
    );
  } else {
    showOcToast(context, error.message);
  }
}

/// Asks for confirmation before blocking someone (§6.2, §6.4).
Future<bool> confirmBlock(BuildContext context, String name) => confirmAction(
  context,
  title: 'Block $name?',
  message:
      "They won't be able to send you messages or friend requests, and you won't see them in servers you share.",
  action: 'Block',
);

/// Asks for confirmation before removing a friend (§6.2, §7.3).
Future<bool> confirmRemoveFriend(BuildContext context, String name) =>
    confirmAction(
      context,
      title: 'Remove $name?',
      message: 'Are you sure you want to remove $name as a friend?',
      action: 'Remove friend',
    );
