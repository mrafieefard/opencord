import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/app_info.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The main area with no servers (§4.13), which is also the first run
/// (§16): the two ways in, joining with an invite or hosting your own.
class NoServersView extends ConsumerWidget {
  const NoServersView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 400),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(OcIcons.forum, size: 48, color: colors.textMuted),
              const SizedBox(height: OcSpace.s16),
              Text(
                'No servers yet',
                textAlign: TextAlign.center,
                style: OcText.title.copyWith(color: colors.text),
              ),
              const SizedBox(height: OcSpace.s6),
              Text(
                'Join a server with an invite link or host your own.',
                textAlign: TextAlign.center,
                style: OcText.body.copyWith(color: colors.textSecondary),
              ),
              const SizedBox(height: OcSpace.s24),
              Wrap(
                alignment: WrapAlignment.center,
                spacing: OcSpace.s8,
                runSpacing: OcSpace.s8,
                children: [
                  OcButton.primary(
                    label: 'Add server',
                    icon: OcIcons.add,
                    onPressed: () => showAddServer(context),
                  ),
                  OcButton(
                    label: 'Host your own',
                    icon: OcIcons.openInNew,
                    onPressed: () async {
                      final opened = await ref.read(linkLauncherProvider)(
                        Uri.parse(hostingGuideUrl),
                      );
                      if (!opened && context.mounted) {
                        showOcToast(
                          context,
                          'No browser could open the guide.',
                        );
                      }
                    },
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
