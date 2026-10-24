import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/typing_dots.dart';
import 'package:opencord/features/home/private_composer.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/home/private_header.dart';
import 'package:opencord/features/home/private_message_list.dart';
import 'package:opencord/features/home/request_bar.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// The conversation view for private messaging (§6.3).
class PrivateChat extends ConsumerStatefulWidget {
  const PrivateChat({super.key, required this.peer});

  final String peer;

  @override
  ConsumerState<PrivateChat> createState() => _PrivateChatState();
}

class _PrivateChatState extends ConsumerState<PrivateChat> {
  final _composerKey = GlobalKey<PrivateComposerState>();
  Timer? _readDebounce;

  @override
  void initState() {
    super.initState();
    _scheduleMarkRead();
    // Coming back to the window reads what arrived while it was away.
    ref.listenManual(windowStatusProvider.select((status) => status.focused), (
      _,
      focused,
    ) {
      if (focused) _scheduleMarkRead();
    });
  }

  @override
  void dispose() {
    _readDebounce?.cancel();
    super.dispose();
  }

  void _scheduleMarkRead() {
    _readDebounce?.cancel();
    _readDebounce = Timer(const Duration(milliseconds: 500), () {
      if (!mounted) return;
      final focused = ref.read(windowStatusProvider).focused;
      if (focused) {
        ref.read(repositoryProvider).markPrivateRead(widget.peer);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final contact = ref.watch(privateContactsProvider).byPeer[widget.peer];
    final name = ref.watch(contactNameProvider(widget.peer));
    final sharedServers = ref.watch(sharedServersProvider(widget.peer));
    final isTyping = ref.watch(privateTypingProvider).containsKey(widget.peer);

    // Listen for new messages to mark read (§6.3)
    ref.listen(privateConversationProvider(widget.peer), (prev, next) {
      if (next.messages.length != prev?.messages.length) {
        _scheduleMarkRead();
      }
    });

    final isBlocked = contact?.blocked ?? false;
    final isRequestMode = contact?.conversation == ConversationStatus.request;

    final anyConnectedSharedServer = sharedServers.any((s) {
      final srv = ref.watch(serverProvider(s.serverKey));
      return srv.connection.phase == ConnectionPhase.connected &&
          srv.data != null;
    });

    Widget bottomBar;
    if (isRequestMode) {
      bottomBar = RequestBar(peer: widget.peer);
    } else if (isBlocked) {
      bottomBar = Container(
        height: 56,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        decoration: BoxDecoration(
          color: colors.surface,
          border: Border(top: BorderSide(color: colors.border)),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              'You blocked $name.',
              style: OcText.body.copyWith(color: colors.textMuted),
            ),
            const SizedBox(width: OcSpace.s12),
            OcButton(
              label: 'Unblock',
              onPressed: () async {
                try {
                  await ref.read(repositoryProvider).unblockUser(widget.peer);
                } on RepoException catch (e) {
                  if (!context.mounted) return;
                  await showPrivateFailure(context, e, name: name);
                }
              },
            ),
          ],
        ),
      );
    } else if (sharedServers.isEmpty) {
      bottomBar = Container(
        height: 56,
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        decoration: BoxDecoration(
          color: colors.surface,
          border: Border(top: BorderSide(color: colors.border)),
        ),
        child: Text(
          'You no longer share a server with $name.',
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    } else if (!anyConnectedSharedServer) {
      bottomBar = Container(
        height: 56,
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        decoration: BoxDecoration(
          color: colors.surface,
          border: Border(top: BorderSide(color: colors.border)),
        ),
        child: Text(
          "You don't share a connected server with $name right now.",
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    } else {
      bottomBar = PrivateComposer(key: _composerKey, peer: widget.peer);
    }

    return Column(
      children: [
        PrivateHeader(peer: widget.peer),
        Expanded(
          child: PrivateMessageList(
            peer: widget.peer,
            onEditMessage: (message) =>
                _composerKey.currentState?.startEditing(message),
          ),
        ),
        if (isTyping)
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s16,
              vertical: OcSpace.s4,
            ),
            child: Row(
              children: [
                TypingDots(color: colors.textSecondary),
                const SizedBox(width: OcSpace.s6),
                Text(
                  '$name is typing…',
                  style: OcText.small.copyWith(color: colors.textSecondary),
                ),
              ],
            ),
          ),
        bottomBar,
      ],
    );
  }
}
