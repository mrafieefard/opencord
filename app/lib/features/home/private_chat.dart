import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/typing_dots.dart';
import 'package:opencord/features/home/key_banners.dart';
import 'package:opencord/features/home/private_composer.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/home/private_header.dart';
import 'package:opencord/features/home/private_message_list.dart';
import 'package:opencord/features/home/request_bar.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

/// The conversation view for private messaging (§6.3).
class PrivateChat extends ConsumerStatefulWidget {
  const PrivateChat({
    super.key,
    required this.peer,
    this.onOpenSidebar,
    this.includeComposer = true,
  });

  final String peer;
  final VoidCallback? onOpenSidebar;
  final bool includeComposer;

  @override
  ConsumerState<PrivateChat> createState() => _PrivateChatState();
}

class _PrivateChatState extends ConsumerState<PrivateChat> {
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
    // Here, above the header and the bottom bar: a provider that first
    // refreshes in one of them can't tell the other during the build.
    ref.watch(sharedServersProvider(widget.peer));
    final isTyping = ref.watch(privateTypingProvider).containsKey(widget.peer);

    // Listen for new messages to mark read (§6.3)
    ref.listen(privateConversationProvider(widget.peer), (prev, next) {
      if (next.messages.length != prev?.messages.length) {
        _scheduleMarkRead();
      }
    });

    final isBlocked = contact?.blocked ?? false;
    final isRequestMode = contact?.conversation == ConversationStatus.request;

    final composerKey = ref.watch(privateComposerKeyProvider(widget.peer));

    return Column(
      children: [
        PrivateHeader(peer: widget.peer, onOpenSidebar: widget.onOpenSidebar),
        Expanded(
          child: PrivateMessageList(
            // Each chat scrolls on its own.
            key: ValueKey(widget.peer),
            peer: widget.peer,
            onEditMessage: (message) =>
                composerKey.currentState?.startEditing(message),
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
        if (!isRequestMode && !isBlocked) KeyBanners(peer: widget.peer),
        if (widget.includeComposer) ...[
          const SizedBox(height: OcFloats.gap),
          ConstrainedBox(
            constraints: const BoxConstraints(minHeight: OcFloats.bottom),
            child: OcFloat(
              key: ShellKeys.composer,
              color: colors.sidebar,
              child: PrivateChatBottomBar(peer: widget.peer),
            ),
          ),
        ],
      ],
    );
  }
}

/// A chat's composer: it sits in the composer float, apart from the message
/// list that asks it to edit.
final privateComposerKeyProvider = Provider.autoDispose
    .family<GlobalKey<PrivateComposerState>, String>(
      (ref, peer) => GlobalKey<PrivateComposerState>(),
    );

/// The composer / blocked / request bar for private chat, for the composer float (§2.1).
class PrivateChatBottomBar extends ConsumerWidget {
  const PrivateChatBottomBar({super.key, required this.peer});

  final String peer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final contact = ref.watch(privateContactsProvider).byPeer[peer];
    final name = ref.watch(contactNameProvider(peer));
    final sharedServers = ref.watch(sharedServersProvider(peer));
    final isBlocked = contact?.blocked ?? false;
    final isRequestMode = contact?.conversation == ConversationStatus.request;

    final anyConnectedSharedServer = sharedServers.any((s) {
      final srv = ref.watch(serverProvider(s.serverKey));
      return srv.connection.phase == ConnectionPhase.connected &&
          srv.data != null;
    });

    final relayKey = contact?.relayServerKey;
    final relayServer = relayKey != null
        ? ref.watch(serverProvider(relayKey))
        : null;
    final isRelayConnected =
        relayServer != null &&
        relayServer.connection.phase == ConnectionPhase.connected &&
        relayServer.data != null;

    final hasSharedServer = sharedServers.isNotEmpty || relayKey != null;
    final anyConnected = anyConnectedSharedServer || isRelayConnected;

    if (isRequestMode) {
      return RequestBar(peer: peer);
    } else if (isBlocked) {
      return Container(
        constraints: const BoxConstraints(minHeight: privateBarHeight),
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        alignment: Alignment.center,
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
                  await ref.read(repositoryProvider).unblockUser(peer);
                } on RepoException catch (e) {
                  if (!context.mounted) return;
                  await showPrivateFailure(context, e, name: name);
                }
              },
            ),
          ],
        ),
      );
    } else if (!hasSharedServer) {
      return Container(
        constraints: const BoxConstraints(minHeight: privateBarHeight),
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        child: Text(
          'You no longer share a server with $name.',
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    } else if (contact != null && !contact.reachable) {
      return Container(
        constraints: const BoxConstraints(minHeight: privateBarHeight),
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        child: Text(
          'Nothing you write would reach $name until you compare the words '
          'or use the new keys.',
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    } else if (!anyConnected) {
      return Container(
        constraints: const BoxConstraints(minHeight: privateBarHeight),
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
        child: Text(
          "You don't share a connected server with $name right now.",
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    } else {
      return PrivateComposer(
        key: ref.watch(privateComposerKeyProvider(peer)),
        peer: peer,
      );
    }
  }
}
