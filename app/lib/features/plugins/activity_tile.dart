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
import 'package:opencord/ui/widgets/oc_icon_button.dart';
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
            _Bar(tile: this, height: _barHeight, onClose: () => _close(ref)),
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
/// Open. A tile too short for all of it drops the icon first, then who has
/// it open, then who started it; the name and Open never shrink.
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

  static const double _iconSize = 40;
  static const double _openHeight = 28;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final session = tile.session;
    final nameStyle = OcText.bodyStrong.copyWith(color: colors.text);
    final scaler = MediaQuery.textScalerOf(context);
    double line(TextStyle style) =>
        scaler.scale(style.fontSize!) * (style.height ?? 1);
    final name = line(nameStyle);
    final small = line(OcText.small);
    // How tall the column is with each line it can drop, fewest first.
    final core = name + OcSpace.s10 + _openHeight;
    final withStarter = core + OcSpace.s2 + small;
    final withViewers = withStarter + small;
    final withIcon = withViewers + _iconSize + OcSpace.s8;
    return LayoutBuilder(
      builder: (context, constraints) {
        final room = constraints.hasBoundedHeight
            ? constraints.maxHeight - 2 * OcSpace.s4
            : double.infinity;
        final icon = withIcon <= room;
        final viewers = withViewers <= room;
        final starterLine = withStarter <= room;
        return Padding(
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
          // Shorter than even the name and Open, the column runs past the
          // tile's edges and the frame clips it; nothing scales.
          child: OverflowBox(
            minHeight: 0,
            maxHeight: double.infinity,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (icon) ...[
                  PluginActivityIcon(
                    serverKey: tile.serverKey,
                    pluginId: session.pluginId,
                    icon: session.icon,
                    size: _iconSize,
                    radius: 10,
                  ),
                  const SizedBox(height: OcSpace.s8),
                ],
                EllipsisText(session.name, style: nameStyle),
                if (starterLine) ...[
                  const SizedBox(height: OcSpace.s2),
                  EllipsisText(
                    'Started by $starter',
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
                if (viewers)
                  EllipsisText(
                    _viewers,
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                const SizedBox(height: OcSpace.s10),
                OcButton.primary(
                  label: 'Open',
                  dense: true,
                  height: _openHeight,
                  onPressed: onOpen,
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

/// Opened: across the top, the icon, the name, Make large or Back to grid,
/// and Close. Narrower than [_wordsFrom], the two buttons are icons with
/// the same names as tooltips, so the bar never overflows.
class _Bar extends StatelessWidget {
  const _Bar({required this.tile, required this.height, required this.onClose});

  final ActivityTile tile;
  final double height;
  final VoidCallback onClose;

  /// The widest the bar gets with the words: padding, the icon, a little
  /// of the name, Back to grid and Close.
  static const double _wordsFrom = 200;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final session = tile.session;
    final focusLabel = tile.focused ? 'Back to grid' : 'Make large';
    return Container(
      height: height,
      padding: const EdgeInsets.only(left: OcSpace.s8, right: OcSpace.s2),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: colors.border)),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final words = constraints.maxWidth >= _wordsFrom;
          return Row(
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
              if (words) ...[
                _BarButton(label: focusLabel, onPressed: tile.onFocus),
                _BarButton(label: 'Close', onPressed: onClose),
              ] else ...[
                OcIconButton(
                  icon: tile.focused
                      ? OcIcons.closeFullscreen
                      : OcIcons.openInFull,
                  tooltip: focusLabel,
                  size: OcIconButtonSize.bar,
                  onPressed: tile.onFocus,
                ),
                OcIconButton(
                  icon: OcIcons.close,
                  tooltip: 'Close',
                  size: OcIconButtonSize.bar,
                  onPressed: onClose,
                ),
              ],
            ],
          );
        },
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
