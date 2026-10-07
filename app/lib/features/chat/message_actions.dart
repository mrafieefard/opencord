import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/chat/reaction_pill.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// `opencord://host:port/c/<channel>/<message>`.
String messageLink(String serverKey, int channelId, int messageId) =>
    '${channelLink(serverKey, channelId)}/$messageId';

/// A message as copied: markdown kept, mentions written out by name.
String copyableText(
  String content, {
  required String? Function(int id) user,
  required String? Function(int id) channel,
}) => content
    .replaceAllMapped(
      RegExp(r'<@(\d+)>'),
      (m) => '@${user(int.parse(m[1]!)) ?? 'unknown'}',
    )
    .replaceAllMapped(
      RegExp(r'<#(\d+)>'),
      (m) => '#${channel(int.parse(m[1]!)) ?? 'unknown'}',
    );

/// What can be done to a message and doing it: the context menu, the hover
/// bar and double-click all go through here (§4.5).
class MessageActions {
  const MessageActions({
    required this.context,
    required this.ref,
    required this.channel,
    required this.controller,
  });

  final BuildContext context;
  final WidgetRef ref;
  final ChannelRef channel;
  final ChatController controller;

  ServerData? get _data => ref.read(serverProvider(channel.server)).data;

  RepoCapabilities get _capabilities =>
      ref.read(repositoryProvider).capabilities;

  Permissions get _held =>
      _data?.permissionsIn(channel.channel) ?? Permissions.none;

  bool _own(Message message) => message.authorId == _data?.self.id;

  bool _sent(Message message) => message.sendState == SendState.sent;

  bool canReply(Message message) =>
      _sent(message) &&
      !message.isSystem &&
      _held.has(Permissions.sendMessages);

  bool canReact(Message message) =>
      _capabilities.reactions && _sent(message) && !message.isSystem;

  bool canEdit(Message message) =>
      _own(message) && _sent(message) && !message.isSystem;

  bool canPin(Message message) =>
      _capabilities.pins &&
      _sent(message) &&
      !message.isSystem &&
      _held.has(Permissions.manageMessages);

  bool canDelete(Message message) =>
      _sent(message) &&
      (_own(message) || _held.has(Permissions.manageMessages));

  ComposerNotifier get _composer =>
      ref.read(composerProvider(channel).notifier);

  ChannelMessagesNotifier get _messages =>
      ref.read(channelMessagesProvider(channel).notifier);

  void _toast(String text) {
    if (context.mounted) showOcToast(context, text);
  }

  Future<void> _guard(Future<void> Function() action) async {
    try {
      await action();
    } on RepoException catch (error) {
      _toast(error.message);
    }
  }

  void reply(Message message) {
    _composer.reply(message);
    controller.focusComposer();
  }

  void edit(Message message) {
    _composer.edit(message);
    controller.focusComposer();
  }

  /// Opens the quick reactions by [anchor] and toggles the chosen one.
  Future<void> react(Message message, {required Rect anchor}) async {
    final emoji = await pickReaction(context, anchor: anchor);
    if (emoji != null) await toggleReaction(message, emoji);
  }

  Future<void> toggleReaction(Message message, String emoji) =>
      _guard(() => _messages.toggleReaction(message.id, emoji));

  void copyText(Message message, {String? selection}) {
    final data = _data;
    Clipboard.setData(
      ClipboardData(
        text:
            selection ??
            copyableText(
              message.content,
              user: (id) => data?.members[id]?.displayName,
              channel: (id) => data?.channels[id]?.name,
            ),
      ),
    );
    _toast('Copied');
  }

  void copyLink(Message message) {
    Clipboard.setData(
      ClipboardData(
        text: messageLink(channel.server, channel.channel, message.id),
      ),
    );
    _toast('Link copied');
  }

  void copyId(Message message) {
    Clipboard.setData(ClipboardData(text: '${message.id}'));
    _toast('ID copied');
  }

  /// Pins straight away; unpinning offers Undo (§16).
  Future<void> togglePin(Message message) => _guard(() async {
    final repository = ref.read(repositoryProvider);
    await repository.setPinned(
      channel.server,
      channel.channel,
      message.id,
      pinned: !message.pinned,
    );
    if (!message.pinned || !context.mounted) return;
    showOcToast(
      context,
      'Message unpinned',
      actionLabel: 'Undo',
      onAction: () => repository.setPinned(
        channel.server,
        channel.channel,
        message.id,
        pinned: true,
      ),
    );
  });

  void retry(Message message) {
    if (message.nonce case final nonce?) _messages.retry(nonce);
  }

  /// A failed send never reached anyone, so it goes without asking.
  void discard(Message message) {
    if (message.nonce case final nonce?) _messages.discard(nonce);
  }

  /// Asks first, for your own messages too (§4.5).
  Future<void> delete(Message message) async {
    final data = _data;
    final confirmed = await showOcDialog<bool>(
      context: context,
      builder: (context) => _DeleteDialog(
        author: data?.members[message.authorId]?.displayName ?? 'Unknown user',
        text: previewText(
          message.content,
          user: (id) => data?.members[id]?.displayName,
          channel: (id) => data?.channels[id]?.name,
        ),
        own: _own(message),
      ),
    );
    if (confirmed != true) return;
    await _guard(() => _messages.delete(message.id));
  }

  /// The context menu (§4.5): Reply · Add reaction · Edit · Copy text ·
  /// Pin · Copy message link · Copy message ID · — · Delete, each only when
  /// it applies. [selection] is text selected inside the message.
  List<OcMenuEntry> menu(
    Message message, {
    required Rect anchor,
    String? selection,
  }) {
    if (message.sendState == SendState.failed) {
      return [
        OcMenuItem(
          label: 'Retry',
          icon: OcIcons.refresh,
          onSelected: () => retry(message),
        ),
        OcMenuItem(
          label: 'Copy text',
          icon: OcIcons.contentCopy,
          onSelected: () => copyText(message),
        ),
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Delete',
          icon: OcIcons.delete,
          onSelected: () => discard(message),
        ),
      ];
    }
    final sent = _sent(message);
    return [
      if (selection != null && selection.isNotEmpty)
        OcMenuItem(
          label: 'Copy selection',
          icon: OcIcons.contentCopy,
          onSelected: () => copyText(message, selection: selection),
        ),
      if (canReply(message))
        OcMenuItem(
          label: 'Reply',
          icon: OcIcons.reply,
          onSelected: () => reply(message),
        ),
      if (canReact(message))
        OcMenuItem(
          label: 'Add reaction',
          icon: OcIcons.addReaction,
          onSelected: () => react(message, anchor: anchor),
        ),
      if (canEdit(message))
        OcMenuItem(
          label: 'Edit',
          icon: OcIcons.edit,
          onSelected: () => edit(message),
        ),
      if (!message.isSystem)
        OcMenuItem(
          label: 'Copy text',
          icon: OcIcons.contentCopy,
          onSelected: () => copyText(message),
        ),
      if (canPin(message))
        OcMenuItem(
          label: message.pinned ? 'Unpin' : 'Pin',
          icon: OcIcons.pushPin,
          onSelected: () => togglePin(message),
        ),
      if (sent) ...[
        OcMenuItem(
          label: 'Copy message link',
          icon: OcIcons.link,
          onSelected: () => copyLink(message),
        ),
        OcMenuItem(
          label: 'Copy message ID',
          icon: OcIcons.badge,
          onSelected: () => copyId(message),
        ),
      ],
      if (canDelete(message)) ...[
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Delete',
          icon: OcIcons.delete,
          onSelected: () => delete(message),
        ),
      ],
    ];
  }

  Future<void> showMenu(
    Message message, {
    required Offset position,
    String? selection,
  }) {
    final entries = menu(
      message,
      anchor: Rect.fromCenter(center: position, width: 1, height: 1),
      selection: selection,
    );
    if (entries.isEmpty) return Future.value();
    return showOcMenu(context: context, position: position, entries: entries);
  }
}

class _DeleteDialog extends StatelessWidget {
  const _DeleteDialog({
    required this.author,
    required this.text,
    required this.own,
  });

  final String author;
  final String text;
  final bool own;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: 'Delete message?',
      actions: [
        OcButton(
          label: 'Cancel',
          onPressed: () => Navigator.pop(context, false),
        ),
        OcButton.primary(
          label: 'Delete',
          autofocus: true,
          onPressed: () => Navigator.pop(context, true),
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            own
                ? 'This deletes your message for everyone.'
                : 'This deletes $author’s message for everyone.',
          ),
          const SizedBox(height: OcSpace.s12),
          Container(
            padding: const EdgeInsets.all(OcSpace.s10),
            decoration: BoxDecoration(
              color: colors.chat,
              borderRadius: BorderRadius.circular(OcRadius.quote),
              border: Border(left: BorderSide(color: colors.text, width: 2)),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  author,
                  style: OcText.small.copyWith(
                    fontWeight: FontWeight.w600,
                    color: colors.text,
                  ),
                ),
                Text(
                  text,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.small.copyWith(color: colors.textSecondary),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
