import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// The member panel (§4.7).
class MemberPanel extends ConsumerWidget {
  const MemberPanel({
    super.key,
    required this.onClose,
    this.trailingControls = false,
  });

  final VoidCallback onClose;

  /// Carries the right-hand window controls (§3.1).
  final bool trailingControls;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    final members = server == null
        ? null
        : ref.watch(
            serverProvider(server).select((state) => state.data?.members),
          );
    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            trailingControls: trailingControls,
            padding: const EdgeInsets.only(
              left: OcSpace.s16,
              right: OcSpace.s8,
            ),
            child: Row(
              children: [
                Expanded(
                  child: IgnorePointer(
                    child: Text(
                      'Members',
                      style: OcText.header.copyWith(color: colors.text),
                    ),
                  ),
                ),
                // Window buttons beside it would make two × in a row; the
                // list still toggles from the chat header.
                if (!trailingControls ||
                    !showsTrailingWindowControls(context, ref))
                  OcIconButton(
                    icon: OcIcons.close,
                    tooltip: 'Close member list',
                    size: OcIconButtonSize.compact,
                    onPressed: onClose,
                  ),
              ],
            ),
          ),
          Expanded(
            child: ListView(
              padding: const EdgeInsets.all(OcSpace.s8),
              children: [
                for (final member in members?.values ?? const <Member>[])
                  Padding(
                    padding: const EdgeInsets.all(OcSpace.s8),
                    child: Text(
                      member.displayName,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.body.copyWith(color: colors.textSecondary),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
