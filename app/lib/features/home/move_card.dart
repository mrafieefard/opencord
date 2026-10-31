import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Card shown when a peer moved the private chat to another server (§6.3, §8).
class MoveCard extends ConsumerWidget {
  const MoveCard({
    super.key,
    required this.peer,
    required this.messageId,
    required this.serverName,
    this.serverKey,
  });

  final String peer;
  final String messageId;
  final String serverName;
  final String? serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(peer));

    final key = serverKey;
    // Already moved there (this device moved too, or another of its
    // devices did): nothing to offer.
    final here =
        key != null &&
        ref.watch(
          privateContactsProvider.select(
            (state) => state.byPeer[peer]?.relayServerKey == key,
          ),
        );
    // Move too needs the server connected on this device with them in it,
    // known by their identity key: names aren't unique.
    final canMoveToo =
        !here &&
        key != null &&
        ref
            .watch(sharedServersProvider(peer))
            .any((server) => server.serverKey == key);

    return Container(
      margin: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s8,
      ),
      padding: const EdgeInsets.all(OcSpace.s12),
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(OcRadius.bubble),
        border: Border.all(color: colors.border),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              canMoveToo || here
                  ? '$name moved this chat to $serverName'
                  : "$name moved this chat to $serverName, a server this device isn't on.",
              style: OcText.body.copyWith(color: colors.text),
            ),
          ),
          if (canMoveToo) ...[
            const SizedBox(width: OcSpace.s12),
            OcButton.primary(
              label: 'Move too',
              onPressed: () async {
                try {
                  await ref
                      .read(repositoryProvider)
                      .moveConversationToo(peer, messageId);
                } on RepoException catch (e) {
                  if (!context.mounted) return;
                  await showPrivateFailure(
                    context,
                    e,
                    name: name,
                    server: serverName,
                    action: PrivateAction.move,
                  );
                }
              },
            ),
          ],
        ],
      ),
    );
  }
}
