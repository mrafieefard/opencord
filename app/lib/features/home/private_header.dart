import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popup_route.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Top header in a private conversation (§6.3).
class PrivateHeader extends ConsumerWidget {
  const PrivateHeader({super.key, required this.peer});

  final String peer;

  void _showFingerprint(
    BuildContext context,
    String name,
    String fingerprint,
  ) {
    final colors = context.oc;
    showOcDialog<void>(
      context: context,
      builder: (context) => OcDialog(
        title: 'End-to-end encryption',
        actions: [
          OcButton.primary(
            label: 'Done',
            autofocus: true,
            onPressed: () => Navigator.pop(context),
          ),
        ],
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Only you and $name can read these messages.'),
            const SizedBox(height: OcSpace.s12),
            Row(
              children: [
                Expanded(
                  child: SelectableText(
                    fingerprint.replaceAll('-', ' '),
                    style: OcText.mono.copyWith(color: colors.text),
                  ),
                ),
                OcIconButton(
                  icon: OcIcons.contentCopy,
                  tooltip: 'Copy fingerprint',
                  size: OcIconButtonSize.compact,
                  onPressed: () {
                    Clipboard.setData(ClipboardData(text: fingerprint));
                    showOcToast(context, 'Fingerprint copied');
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  void _showRelayMenu(
    BuildContext context,
    WidgetRef ref,
    String name,
    String? currentRelayKey,
    List<SharedServerView> sharedServers,
  ) {
    final rect = globalRectOf(context);
    showOcMenu(
      context: context,
      position: Offset(rect.left, rect.bottom + OcSpace.s4),
      entries: [
        for (final server in sharedServers)
          OcMenuItem(
            label: server.name,
            checked: server.serverKey == currentRelayKey,
            onSelected: () async {
              if (server.serverKey == currentRelayKey) return;
              try {
                await ref
                    .read(repositoryProvider)
                    .moveConversation(peer, server.serverKey);
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(
                  context,
                  e,
                  name: name,
                  server: server.name,
                  action: PrivateAction.move,
                );
              }
            },
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final contact = ref.watch(privateContactsProvider).byPeer[peer];
    final name = ref.watch(contactNameProvider(peer));
    final presence = ref.watch(contactPresenceProvider(peer));
    final sharedServers = ref.watch(sharedServersProvider(peer));

    final presenceLabel = switch (presence) {
      Presence.online => 'Online',
      Presence.idle => 'Idle',
      Presence.doNotDisturb => 'Do not disturb',
      Presence.offline => 'Offline',
    };

    final relayKey = contact?.relayServerKey;
    final relayServer = relayKey != null
        ? ref.watch(serverProvider(relayKey))
        : null;
    final relayServerName = relayServer?.data?.info.name ??
        sharedServers
            .where((s) => s.serverKey == relayKey)
            .firstOrNull
            ?.name ??
        'Server';

    final isRelayConnected = relayServer != null &&
        relayServer.connection.phase == ConnectionPhase.connected &&
        relayServer.data != null;

    final unjoinedRelay = relayKey == null &&
        contact?.conversation == ConversationStatus.accepted;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          height: 60,
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
          decoration: BoxDecoration(
            border: Border(bottom: BorderSide(color: colors.border)),
          ),
          child: Row(
            children: [
              ContactAvatar(peer: peer, presence: presence),
              const SizedBox(width: OcSpace.s12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Text(
                      name,
                      style: OcText.header.copyWith(color: colors.text),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    Text(
                      presenceLabel,
                      style: OcText.small.copyWith(color: colors.textMuted),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),

              // End-to-end encryption lock button (§6.3)
              OcIconButton(
                icon: OcIcons.lock,
                tooltip:
                    'End-to-end encrypted. Only you and $name can read these messages.',
                onPressed: () => _showFingerprint(
                  context,
                  name,
                  contact?.fingerprint ?? peer,
                ),
              ),

              const SizedBox(width: OcSpace.s8),

              // Relay chip (§6.3)
              Builder(
                builder: (btnContext) => Hoverable(
                  onTap: sharedServers.isNotEmpty
                      ? () => _showRelayMenu(
                            btnContext,
                            ref,
                            name,
                            relayKey,
                            sharedServers,
                          )
                      : null,
                  semanticLabel: 'Relay server',
                  builder: (context, state) => Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s8,
                      vertical: OcSpace.s4,
                    ),
                    decoration: BoxDecoration(
                      color: state.active ? colors.hover : colors.surface,
                      borderRadius:
                          BorderRadius.circular(OcRadius.bubbleGrouped),
                      border: Border.all(
                        color: !isRelayConnected && relayKey != null
                            ? colors.textSecondary
                            : colors.border,
                      ),
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (relayKey != null)
                          ServerAvatar(
                            serverKey: relayKey,
                            name: relayServerName,
                            size: 16,
                            borderRadius: 4,
                          ),
                        const SizedBox(width: OcSpace.s6),
                        Text(
                          'via $relayServerName',
                          style: OcText.small.copyWith(
                            color: !isRelayConnected && relayKey != null
                                ? colors.textSecondary
                                : colors.text,
                          ),
                        ),
                        const SizedBox(width: OcSpace.s4),
                        Icon(
                          OcIcons.expandMore,
                          size: 16,
                          color: colors.textMuted,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),

        // Unjoined server warning line (§6.3)
        if (unjoinedRelay)
          Container(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s16,
              vertical: OcSpace.s6,
            ),
            color: colors.hover,
            child: Text(
              'This chat goes through $relayServerName, which this device hasn\'t joined.',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ),
      ],
    );
  }
}
