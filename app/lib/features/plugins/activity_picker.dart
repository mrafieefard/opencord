import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The activity a row of the picker chose, and the call it was chosen in.
typedef _Pick = ({String server, int channel, PluginActivity activity});

/// Opens the activity picker (design §15.2) above [anchor], a global
/// rectangle, for this device's call. The row pressed starts its activity
/// there once the picker has closed; why it couldn't shows as a toast.
Future<void> showActivityPicker(
  BuildContext context, {
  required Rect anchor,
}) async {
  final pick = await showPopover<_Pick>(
    context: context,
    anchor: anchor,
    side: PopoverSide.above,
    padding: EdgeInsets.zero,
    builder: (_) => const ActivityPicker(),
  );
  if (pick == null || !context.mounted) return;
  final repository = ProviderScope.containerOf(
    context,
    listen: false,
  ).read(repositoryProvider);
  try {
    // A running one comes back as it is: the server answers with it.
    await repository.startActivity(
      pick.server,
      pick.channel,
      pick.activity.pluginId,
      pick.activity.activityKey,
    );
  } on RepoException catch (error) {
    if (!context.mounted) return;
    showOcToast(context, switch (error.kind) {
      RepoErrorKind.pluginUnavailable =>
        "This activity isn't available right now.",
      RepoErrorKind.pluginLimit =>
        'This call already has $activitySessionsPerCall activities.',
      RepoErrorKind.forbidden =>
        "You don't have permission to start activities here.",
      _ => error.message,
    });
  }
}

/// The registered activities of this server's running plugins, one row
/// each; pressing one closes the picker with it.
class ActivityPicker extends ConsumerWidget {
  const ActivityPicker({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final voice = ref.watch(voiceSessionProvider);
    final server = voice.serverKey;
    final channel = voice.channelId;
    if (server == null || channel == null) {
      return const SizedBox(
        width: 320,
        child: Padding(
          padding: EdgeInsets.all(OcSpace.s16),
          child: Text('Join a voice channel to start an activity.'),
        ),
      );
    }
    final activities = [
      ...?ref.watch(serverProvider(server)).data?.pluginActivities,
    ]..sort(_byPluginThenName);
    final running = [
      for (final session in ref.watch(activitySessionsProvider(server)).values)
        if (session.channelId == channel) session,
    ];
    bool isRunning(PluginActivity activity) => running.any(
      (session) =>
          session.pluginId == activity.pluginId &&
          session.activityKey == activity.activityKey,
    );
    final full = running.length >= activitySessionsPerCall;

    return ConstrainedBox(
      constraints: const BoxConstraints.tightFor(
        width: 320,
      ).copyWith(maxHeight: 360),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              OcSpace.s16,
              OcSpace.s12,
              OcSpace.s16,
              OcSpace.s4,
            ),
            child: Text(
              'Activities',
              style: OcText.header.copyWith(color: colors.text),
            ),
          ),
          Flexible(
            child: ListView(
              shrinkWrap: true,
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s8,
                OcSpace.s4,
                OcSpace.s8,
                OcSpace.s8,
              ),
              children: [
                for (final activity in activities)
                  _ActivityRow(
                    server: server,
                    activity: activity,
                    running: isRunning(activity),
                    enabled: !full || isRunning(activity),
                    onPressed: () => Navigator.of(context).pop<_Pick>((
                      server: server,
                      channel: channel,
                      activity: activity,
                    )),
                  ),
              ],
            ),
          ),
          if (full)
            Padding(
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s16,
                0,
                OcSpace.s16,
                OcSpace.s12,
              ),
              child: Text(
                'This call already has $activitySessionsPerCall activities.',
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ),
        ],
      ),
    );
  }
}

int _byPluginThenName(PluginActivity a, PluginActivity b) {
  final byPlugin = a.pluginName.toLowerCase().compareTo(
    b.pluginName.toLowerCase(),
  );
  if (byPlugin != 0) return byPlugin;
  return a.name.toLowerCase().compareTo(b.name.toLowerCase());
}

class _ActivityRow extends StatelessWidget {
  const _ActivityRow({
    required this.server,
    required this.activity,
    required this.running,
    required this.enabled,
    required this.onPressed,
  });

  final String server;
  final PluginActivity activity;
  final bool running;
  final bool enabled;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final subtitle = activity.description.isEmpty
        ? activity.pluginName
        : activity.description;
    return Hoverable(
      onTap: enabled ? onPressed : null,
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      semanticLabel: running ? '${activity.name}, running' : activity.name,
      focusRadius: BorderRadius.circular(OcRadius.row),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        curve: OcMotion.curve,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s8,
          vertical: OcSpace.s6,
        ),
        decoration: BoxDecoration(
          color: enabled && state.active ? colors.selected : Colors.transparent,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Opacity(
          opacity: enabled ? 1 : 0.4,
          child: Row(
            children: [
              PluginActivityIcon(
                serverKey: server,
                pluginId: activity.pluginId,
                icon: activity.icon,
                size: 32,
                radius: 8,
              ),
              const SizedBox(width: OcSpace.s10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    EllipsisText(
                      activity.name,
                      style: OcText.bodyStrong.copyWith(color: colors.text),
                    ),
                    EllipsisText(
                      subtitle,
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  ],
                ),
              ),
              if (running) ...[
                const SizedBox(width: OcSpace.s8),
                Text(
                  'Running',
                  style: OcText.small.copyWith(color: colors.textSecondary),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// An activity's icon: the plugin's asset at [icon], or the extension glyph
/// when it has none, while it loads, or when it fails to load. Square,
/// [size] wide, with [radius] corners.
class PluginActivityIcon extends ConsumerWidget {
  const PluginActivityIcon({
    super.key,
    required this.serverKey,
    required this.pluginId,
    required this.icon,
    required this.size,
    required this.radius,
  });

  final String serverKey;
  final String pluginId;

  /// A path under the plugin's `assets/`, or empty.
  final String icon;
  final double size;
  final double radius;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final file = icon.isEmpty
        ? null
        : ref
              .watch(
                pluginAssetFileProvider(
                  PluginAssetRef(
                    serverKey: serverKey,
                    pluginId: pluginId,
                    path: icon,
                  ),
                ),
              )
              .value;
    final fallback = Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: colors.selected,
        borderRadius: BorderRadius.circular(radius),
      ),
      alignment: Alignment.center,
      child: Icon(OcIcons.extension, size: size / 2, color: colors.text),
    );
    if (file == null || file.isEmpty) return fallback;
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: Image.file(
        File(file),
        width: size,
        height: size,
        fit: BoxFit.cover,
        errorBuilder: (_, _, _) => fallback,
      ),
    );
  }
}
