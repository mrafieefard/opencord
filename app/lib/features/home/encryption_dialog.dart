import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

/// 16 words in a 4×4 grid, read left to right, top to bottom (design
/// 2026-11-01 §9).
class WordGrid extends StatelessWidget {
  const WordGrid({super.key, required this.words});

  final List<String> words;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var row = 0; row < 4; row++)
          Padding(
            padding: EdgeInsets.only(top: row == 0 ? 0 : OcSpace.s6),
            child: Row(
              children: [
                for (var column = 0; column < 4; column++) ...[
                  if (column > 0) const SizedBox(width: OcSpace.s6),
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        vertical: OcSpace.s8,
                        horizontal: OcSpace.s6,
                      ),
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: colors.surface,
                        borderRadius: BorderRadius.circular(OcRadius.quote),
                        border: Border.all(color: colors.border),
                      ),
                      child: Text(
                        words.length > row * 4 + column
                            ? words[row * 4 + column]
                            : '',
                        style: OcText.mono.copyWith(color: colors.text),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ),
      ],
    );
  }
}

/// The words, or a spinner while they load.
class _Words extends StatelessWidget {
  const _Words({required this.words});

  final Future<KeyWords> words;

  @override
  Widget build(BuildContext context) => FutureBuilder<KeyWords>(
    future: words,
    builder: (context, snapshot) => switch (snapshot) {
      AsyncSnapshot(hasData: true, :final data?) => WordGrid(words: data.words),
      AsyncSnapshot(hasError: true) => Text(
        "The words couldn't be worked out.",
        style: OcText.body.copyWith(color: context.oc.textSecondary),
      ),
      _ => const SizedBox(
        height: 120,
        child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
      ),
    },
  );
}

/// End-to-end encryption for the chat with [peer] (§9.3). With [newKeys],
/// the words are made from the other chat key their devices offer.
Future<void> showEncryptionDialog(
  BuildContext context,
  WidgetRef ref, {
  required String peer,
  required String name,
  bool newKeys = false,
}) {
  final repo = ref.read(repositoryProvider);
  final words = repo.privateWords(peer, newKeys: newKeys);
  return showOcDialog<void>(
    context: context,
    builder: (dialogContext) => Consumer(
      builder: (context, ref, _) {
        final colors = context.oc;
        final verification =
            ref.watch(
              privateContactsProvider.select(
                (contacts) => contacts.byPeer[peer]?.verification,
              ),
            ) ??
            PrivateVerification.unverified;
        final (status, statusColor) = newKeys
            ? ('New keys', colors.textSecondary)
            : switch (verification) {
                PrivateVerification.unverified => (
                  'Not verified',
                  colors.textSecondary,
                ),
                PrivateVerification.verified => ('Verified', colors.accent),
                PrivateVerification.mismatch => (
                  "Didn't match",
                  colors.recording,
                ),
              };
        Future<void> answer(bool matched) async {
          try {
            // Only what was shown is answered for.
            final shown = await words;
            await repo.verifyPrivate(
              peer,
              matched: matched,
              keys: shown.keys,
              newKeys: newKeys,
            );
            if (dialogContext.mounted) Navigator.pop(dialogContext);
          } on RepoException catch (e) {
            if (!dialogContext.mounted) return;
            await showPrivateFailure(dialogContext, e, name: name);
          }
        }

        return OcDialog(
          title: 'End-to-end encryption',
          actions: [
            OcButton(label: "They don't match", onPressed: () => answer(false)),
            OcButton.primary(
              label: 'They match',
              autofocus: true,
              onPressed: () => answer(true),
            ),
          ],
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                status,
                style: OcText.bodyStrong.copyWith(color: statusColor),
              ),
              const SizedBox(height: OcSpace.s12),
              _Words(words: words),
              const SizedBox(height: OcSpace.s12),
              Text(
                'Compare these words with $name in person or on a call. '
                "If they're the same, only you two can read this chat.",
                style: OcText.body.copyWith(color: colors.textSecondary),
              ),
            ],
          ),
        );
      },
    ),
  );
}

/// The grid two of this person's devices compare (§7.2). [offering]: the
/// other device sent its chat key, and They match takes it; otherwise They
/// match gives it this device's.
Future<void> showPairingDialog(
  BuildContext context,
  WidgetRef ref, {
  required String deviceId,
  required bool offering,
}) {
  final repo = ref.read(repositoryProvider);
  final words = repo.pairingWords(deviceId);
  final device = deviceLabel(deviceId);
  return showOcDialog<void>(
    context: context,
    builder: (dialogContext) {
      final colors = dialogContext.oc;
      Future<void> answer(bool matched) async {
        try {
          final shown = await words;
          if (offering) {
            await repo.acceptKeysFrom(
              deviceId,
              matched: matched,
              keys: shown.keys,
            );
          } else if (matched) {
            await repo.approveOwnDevice(deviceId, keys: shown.keys);
          }
          if (dialogContext.mounted) Navigator.pop(dialogContext);
        } on RepoException catch (e) {
          if (!dialogContext.mounted) return;
          await showPrivateFailure(dialogContext, e, name: device);
        }
      }

      return OcDialog(
        title: offering ? 'Take its keys' : 'Approve this device',
        actions: [
          OcButton(label: "They don't match", onPressed: () => answer(false)),
          OcButton.primary(
            label: 'They match',
            autofocus: true,
            onPressed: () => answer(true),
          ),
        ],
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              offering
                  ? '$device wants to give this device its keys.'
                  : 'Check that $device shows the same words.',
              style: OcText.body.copyWith(color: colors.text),
            ),
            const SizedBox(height: OcSpace.s12),
            _Words(words: words),
            const SizedBox(height: OcSpace.s12),
            Text(
              offering
                  ? 'Only tap They match if both screens show these words.'
                  : 'They match gives it the keys to your chats.',
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
          ],
        ),
      );
    },
  );
}

/// "Your device ABCD": the first four hex digits of its id.
String deviceLabel(String deviceId) =>
    'Your device ${deviceId.substring(0, deviceId.length < 4 ? deviceId.length : 4).toUpperCase()}';
