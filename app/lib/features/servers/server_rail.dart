import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/drag_area.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// The server rail (§4.1): one icon per server with its name underneath.
class ServerRail extends ConsumerWidget {
  const ServerRail({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final servers = ref.watch(serverListProvider);
    final current = ref.watch(currentServerProvider);
    // macOS keeps its traffic lights above the rail (§3.1).
    final trafficLights =
        Theme.of(context).platform == TargetPlatform.macOS &&
        ref.watch(windowChromeProvider) == WindowChrome.custom &&
        !ref.watch(windowStatusProvider.select((status) => status.fullscreen));
    return ColoredBox(
      color: colors.rail,
      child: Column(
        children: [
          if (trafficLights)
            const SizedBox(height: 52, child: WindowDragArea()),
          SizedBox(
            height: OcSize.header,
            child: Stack(
              alignment: Alignment.center,
              children: const [
                Positioned.fill(child: WindowDragArea()),
                OcIconButton(icon: OcIcons.menu, tooltip: 'Settings'),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(bottom: OcSpace.s8),
            child: SizedBox(
              width: 32,
              child: Divider(height: 1, color: colors.border),
            ),
          ),
          Expanded(
            child: ListView(
              padding: EdgeInsets.zero,
              children: [
                for (final server in servers)
                  _RailItem(
                    serverKey: server.key,
                    name: server.name,
                    selected: server.key == current,
                  ),
              ],
            ),
          ),
          const OcIconButton(icon: OcIcons.add, tooltip: 'Add server'),
          const SizedBox(height: OcSpace.s12),
        ],
      ),
    );
  }
}

class _RailItem extends ConsumerWidget {
  const _RailItem({
    required this.serverKey,
    required this.name,
    required this.selected,
  });

  final String serverKey;
  final String name;
  final bool selected;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Hoverable(
      onTap: () => ref.read(navigationProvider.notifier).openServer(serverKey),
      semanticLabel: name,
      selected: selected,
      builder: (context, state) => Padding(
        padding: const EdgeInsets.symmetric(vertical: OcSpace.s4),
        child: Column(
          children: [
            OcAvatar(
              id: serverKey,
              name: name,
              size: OcSize.serverIcon,
              borderRadius: selected || state.active
                  ? OcRadius.serverIconActive
                  : OcRadius.serverIcon,
              inverted: selected,
            ),
            const SizedBox(height: OcSpace.s4),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: OcSpace.s4),
              child: Text(
                name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: OcText.meta.copyWith(
                  color: selected ? colors.text : colors.textMuted,
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
