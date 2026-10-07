import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Servers whose certificate no longer matches the trusted fingerprint
/// (§4.11), with the status that says so.
final identityChangesProvider = Provider<Map<String, ConnectionStatus>>((ref) {
  return {
    for (final server in ref.watch(serverListProvider))
      if (ref.watch(serverProvider(server.key).select((s) => s.connection))
          case final status
          when status.failure == FailureReason.fingerprintChanged)
        server.key: status,
  };
});

/// The blocking "Server identity changed" dialog (§4.11): both
/// fingerprints, and only Disconnect or Forget server.
Future<void> showIdentityChanged(
  BuildContext context, {
  required String serverKey,
  required String name,
  required ConnectionStatus status,
}) => showOcDialog<void>(
  context: context,
  dismissible: false,
  builder: (context) => IdentityChangedDialog(
    serverKey: serverKey,
    name: name,
    expected: status.expectedFingerprint,
    presented: status.presentedFingerprint,
  ),
);

class IdentityChangedDialog extends ConsumerWidget {
  const IdentityChangedDialog({
    super.key,
    required this.serverKey,
    required this.name,
    this.expected,
    this.presented,
  });

  final String serverKey;
  final String name;
  final String? expected;
  final String? presented;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    Widget fingerprint(String label, String? hex) => Padding(
      padding: const EdgeInsets.only(top: OcSpace.s16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SectionLabel(label),
          const SizedBox(height: OcSpace.s6),
          Container(
            padding: const EdgeInsets.all(OcSpace.s10),
            decoration: BoxDecoration(
              color: colors.chat,
              borderRadius: BorderRadius.circular(OcRadius.input),
              border: Border.all(color: colors.border),
            ),
            child: SelectableText(
              hex == null ? 'Not known' : groupedFingerprint(hex),
              style: OcText.mono.copyWith(color: colors.text),
            ),
          ),
        ],
      ),
    );
    // Only the two buttons close it (§4.11).
    return PopScope(
      canPop: false,
      child: OcDialog(
        title: 'Server identity changed',
        width: 520,
        actions: [
          OcButton(
            label: 'Disconnect',
            onPressed: () => Navigator.pop(context),
          ),
          OcButton.primary(
            label: 'Forget server',
            onPressed: () async {
              try {
                await ref.read(repositoryProvider).removeServer(serverKey);
              } on RepoException catch (error) {
                if (context.mounted) showOcToast(context, error.message);
              }
              if (context.mounted) Navigator.pop(context);
            },
          ),
        ],
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              '$name no longer shows the identity you trusted. Someone may be '
              'pretending to be it, or its owner replaced its certificate. '
              'Opencord will not connect until you know which: ask the owner, '
              'then forget the server and add it again.',
            ),
            fingerprint('Trusted fingerprint', expected),
            fingerprint('Fingerprint shown now', presented),
          ],
        ),
      ),
    );
  }
}
