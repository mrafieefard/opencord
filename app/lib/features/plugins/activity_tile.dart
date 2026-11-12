import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/plugins/activity_picker.dart';
import 'package:opencord/features/plugins/plugin_area.dart';
import 'package:opencord/ui/motion/flip_box.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// An activity session's tile on the call's stage (plugins M3, §15.2–§15.4).
/// Not opened, it says what runs and offers Open; opened, a bar over the
/// plugin's own screen; in focus mode's strip, only its icon and name.
class ActivityTile extends ConsumerWidget {
  const ActivityTile({
    super.key,
    required this.serverKey,
    required this.session,
    required this.onFocus,
    this.focused = false,
    this.small = false,
  });

  final String serverKey;
  final ActivitySession session;

  /// Shown large in focus mode.
  final bool focused;

  /// In focus mode's strip.
  final bool small;

  /// Focuses the tile, or returns to the grid when it is [focused].
  final VoidCallback onFocus;

  static const double _barHeight = 28;

  void _open(WidgetRef ref) =>
      ref.read(openActivitiesProvider.notifier).open(session.id);

  void _close(WidgetRef ref) =>
      ref.read(openActivitiesProvider.notifier).close(session.id);

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final opened = ref.watch(
      openActivitiesProvider.select((ids) => ids.contains(session.id)),
    );
    final serverData = ref.watch(
      serverProvider(serverKey).select((state) => state.data),
    );
    final canEnd =
        serverData != null &&
        (session.starterId == serverData.self.id ||
            serverData
                .permissionsIn(session.channelId)
                .has(Permissions.manageChannels));
    void menu(Offset position) => showOcMenu(
      context: context,
      position: position,
      entries: [
        if (opened)
          OcMenuItem(
            label: 'Close',
            icon: OcIcons.close,
            onSelected: () => _close(ref),
          )
        else
          OcMenuItem(
            label: 'Open',
            icon: OcIcons.openInNew,
            onSelected: () => _open(ref),
          ),
        if (canEnd) ...[
          const OcMenuDivider(),
          OcMenuItem(
            label: 'End activity',
            icon: OcIcons.stop,
            onSelected: () => _end(context, ref),
          ),
        ],
      ],
    );

    if (small) {
      return Hoverable(
        onTap: onFocus,
        onSecondaryTap: menu,
        semanticLabel: '${session.name}, activity',
        focusRadius: BorderRadius.circular(OcRadius.tile),
        builder: (context, _) => _Frame(child: _Small(tile: this)),
      );
    }
    if (!opened) {
      return Hoverable(
        onTap: () => _open(ref),
        onSecondaryTap: menu,
        semanticLabel: '${session.name}, activity',
        focusRadius: BorderRadius.circular(OcRadius.tile),
        builder: (context, _) => _Frame(
          child: _NotOpened(
            tile: this,
            starter: serverData?.authorOf(session.starterId).name ?? 'Someone',
            onOpen: () => _open(ref),
          ),
        ),
      );
    }
    // Opened: the plugin's screen takes its own presses; the secondary
    // press anywhere on the tile still opens the menu.
    return GestureDetector(
      onSecondaryTapUp: (details) => menu(details.globalPosition),
      child: _Frame(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _Bar(
              tile: this,
              height: _barHeight,
              onClose: () => _close(ref),
            ),
            // Its modals cover only this part (§12.5).
            Expanded(
              child: ClipRect(
                child: PluginArea(place: ActivityPlace(serverKey, session)),
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// Asks, then ends the activity for everyone in the call.
  Future<void> _end(BuildContext context, WidgetRef ref) async {
    final end = await confirmAction(
      context,
      title: 'End ${session.name} for everyone?',
      message: 'It closes for everyone in the call.',
      action: 'End activity',
    );
    if (!end || !context.mounted) return;
    try {
      await ref.read(repositoryProvider).endActivity(serverKey, session.id);
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }
}

/// A tile's frame, as a person's: the surface, a 1 px border, the tile's
/// radius, and what it holds clipped to it.
class _Frame extends StatelessWidget {
  const _Frame({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(OcRadius.tile),
        border: Border.all(color: colors.border),
      ),
      // The tile's frame scales on the way; this comes back after.
      child: FlipBoxContents(child: child),
    );
  }
}

/// Not opened: the icon, the name, who started it, who has it open, and
/// Open. A tile too short for all of it scales it down.
class _NotOpened extends StatelessWidget {
  const _NotOpened({
    required this.tile,
    required this.starter,
    required this.onOpen,
  });

  final ActivityTile tile;
  final String starter;
  final VoidCallback onOpen;

  String get _viewers => switch (tile.session.viewerIds.length) {
    0 => 'No one has it open',
    1 => '1 person has it open',
    final count => '$count people have it open',
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final session = tile.session;
    return LayoutBuilder(
      builder: (context, constraints) => Padding(
        padding: const EdgeInsets.all(OcSpace.s12),
        child: Center(
          child: FittedBox(
            fit: BoxFit.scaleDown,
            child: ConstrainedBox(
              constraints: BoxConstraints(
                maxWidth: math.max(0.0, constraints.maxWidth - 2 * OcSpace.s12),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  PluginActivityIcon(
                    serverKey: tile.serverKey,
                    pluginId: session.pluginId,
                    icon: session.icon,
                    size: 40,
                    radius: 10,
                  ),
                  const SizedBox(height: OcSpace.s8),
                  EllipsisText(
                    session.name,
                    style: OcText.bodyStrong.copyWith(color: colors.text),
                  ),
                  const SizedBox(height: OcSpace.s2),
                  EllipsisText(
                    'Started by $starter',
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                  EllipsisText(
                    _viewers,
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                  const SizedBox(height: OcSpace.s10),
                  OcButton.primary(label: 'Open', dense: true, onPressed: onOpen),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Opened: across the top, the icon, the name, Make large or Back to grid,
/// and Close.
class _Bar extends StatelessWidget {
  const _Bar({required this.tile, required this.height, required this.onClose});

  final ActivityTile tile;
  final double height;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final session = tile.session;
    return Container(
      height: height,
      padding: const EdgeInsets.only(left: OcSpace.s8, right: OcSpace.s2),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: colors.border)),
      ),
      child: Row(
        children: [
          PluginActivityIcon(
            serverKey: tile.serverKey,
            pluginId: session.pluginId,
            icon: session.icon,
            size: 16,
            radius: 4,
          ),
          const SizedBox(width: OcSpace.s6),
          Expanded(
            child: EllipsisText(
              session.name,
              style: OcText.small.copyWith(
                color: colors.text,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          _BarButton(
            label: tile.focused ? 'Back to grid' : 'Make large',
            onPressed: tile.onFocus,
          ),
          _BarButton(label: 'Close', onPressed: onClose),
        ],
      ),
    );
  }
}

class _BarButton extends StatelessWidget {
  const _BarButton({required this.label, required this.onPressed});

  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => OcButton.ghost(
    label: label,
    dense: true,
    height: 22,
    padding: const EdgeInsets.symmetric(horizontal: OcSpace.s6),
    onPressed: onPressed,
  );
}

/// In focus mode's strip: the icon and the name.
class _Small extends StatelessWidget {
  const _Small({required this.tile});

  final ActivityTile tile;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final session = tile.session;
    return Padding(
      padding: const EdgeInsets.all(OcSpace.s6),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          PluginActivityIcon(
            serverKey: tile.serverKey,
            pluginId: session.pluginId,
            icon: session.icon,
            size: 24,
            radius: 6,
          ),
          const SizedBox(height: OcSpace.s4),
          EllipsisText(
            session.name,
            style: OcText.small.copyWith(color: colors.text),
          ),
        ],
      ),
    );
  }
}
