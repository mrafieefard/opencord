import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// The channel sidebar (§4.2).
class ChannelSidebar extends ConsumerWidget {
  const ChannelSidebar({super.key, this.leadingControls = false});

  /// Carries left-hand window controls (§3.1).
  final bool leadingControls;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    final data = server == null
        ? null
        : ref.watch(serverProvider(server).select((state) => state.data));
    final current = ref.watch(currentChannelProvider);
    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            leadingControls: leadingControls,
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
            child: IgnorePointer(
              child: Text(
                data?.info.name ?? '',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.header.copyWith(color: colors.text),
              ),
            ),
          ),
          Expanded(
            child: data == null || server == null
                ? const SizedBox()
                : ListView(
                    padding: const EdgeInsets.symmetric(vertical: OcSpace.s8),
                    children: [
                      for (final group in channelTree(
                        data.channels.values,
                      )) ...[
                        if (group.category != null)
                          SectionLabel(
                            group.category!.name,
                            padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                          ),
                        for (final channel in group.channels)
                          Hoverable(
                            onTap: () => ref
                                .read(navigationProvider.notifier)
                                .openChannel(server, channel.id),
                            semanticLabel: channel.name,
                            selected: channel.id == current,
                            builder: (context, state) => Container(
                              margin: const EdgeInsets.symmetric(
                                horizontal: OcSpace.s8,
                              ),
                              padding: const EdgeInsets.symmetric(
                                horizontal: OcSpace.s10,
                                vertical: OcSpace.s8,
                              ),
                              decoration: BoxDecoration(
                                color: channel.id == current
                                    ? colors.selected
                                    : state.active
                                    ? colors.hover
                                    : null,
                                borderRadius: BorderRadius.circular(
                                  OcRadius.row,
                                ),
                              ),
                              child: Text(
                                channel.kind.isTextLike
                                    ? '# ${channel.name}'
                                    : channel.name,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: OcText.body.copyWith(
                                  color: colors.textSecondary,
                                ),
                              ),
                            ),
                          ),
                      ],
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}
