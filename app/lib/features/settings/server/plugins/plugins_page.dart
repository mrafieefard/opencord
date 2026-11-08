import 'dart:async';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/plugins_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/plugins/review_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

const _pluginZipTypes = XTypeGroup(
  label: 'Plugins',
  extensions: ['zip'],
);

/// Server settings → Plugins (§17.1): drop zone, waiting dev reviews,
/// installed plugins with metrics and run switch.
class ServerPluginsPage extends ConsumerStatefulWidget {
  const ServerPluginsPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerPluginsPage> createState() => _ServerPluginsPageState();
}

class _ServerPluginsPageState extends ConsumerState<ServerPluginsPage> {
  bool _dragging = false;
  Timer? _pollTimer;

  @override
  void initState() {
    super.initState();
    // Load reviews and poll CPU and memory use every 5 seconds while open (§17.1).
    _poll();
    _pollTimer = Timer.periodic(const Duration(seconds: 5), (_) => _poll());
  }

  @override
  void dispose() {
    _pollTimer?.cancel();
    super.dispose();
  }

  Future<void> _poll() async {
    try {
      final plugins = await ref
          .read(repositoryProvider)
          .fetchPlugins(widget.serverKey);
      if (mounted) {
        ref.read(pluginsProvider(widget.serverKey).notifier).set(plugins);
      }
    } catch (_) {}

    try {
      final reviews = await ref
          .read(repositoryProvider)
          .fetchPluginReviews(widget.serverKey);
      if (mounted) {
        ref.read(pluginReviewsProvider(widget.serverKey).notifier).set(reviews);
      }
    } catch (_) {}
  }

  Future<void> _chooseFile() async {
    final file = await openFile(acceptedTypeGroups: const [_pluginZipTypes]);
    if (file == null || !mounted) return;
    final bytes = await file.readAsBytes();
    if (!mounted) return;
    await showReviewDialog(
      context,
      serverKey: widget.serverKey,
      uploadBytes: bytes,
      fileName: file.name,
    );
  }

  Future<void> _onDrop(DropDoneDetails details) async {
    setState(() => _dragging = false);
    if (details.files.isEmpty) return;
    final file = details.files.first;
    final bytes = await file.readAsBytes();
    if (!mounted) return;
    await showReviewDialog(
      context,
      serverKey: widget.serverKey,
      uploadBytes: bytes,
      fileName: file.name,
    );
  }

  Future<void> _toggleRunning(Plugin plugin, bool on) async {
    try {
      await ref
          .read(repositoryProvider)
          .setPluginRunning(widget.serverKey, plugin.id, on);
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    } catch (e) {
      if (mounted) showOcToast(context, e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final pluginsAsync = ref.watch(pluginsProvider(widget.serverKey));
    final plugins = pluginsAsync.value ?? const [];
    final reviews = ref.watch(pluginReviewsProvider(widget.serverKey));
    final devReviews = reviews.values.where((r) => r.dev).toList();

    return SingleChildScrollView(
      padding: const EdgeInsets.all(OcSpace.s24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Drop zone
          DropTarget(
            onDragEntered: (_) => setState(() => _dragging = true),
            onDragExited: (_) => setState(() => _dragging = false),
            onDragDone: _onDrop,
            child: Container(
              padding: const EdgeInsets.symmetric(
                horizontal: OcSpace.s24,
                vertical: OcSpace.s24,
              ),
              decoration: BoxDecoration(
                color: _dragging ? colors.hover : colors.surface,
                borderRadius: BorderRadius.circular(OcRadius.section),
                border: Border.all(
                  color: _dragging ? colors.accent : colors.border,
                  width: _dragging ? 1.5 : 1.0,
                ),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    OcIcons.folderZip,
                    size: 36,
                    color: colors.textSecondary,
                  ),
                  const SizedBox(height: OcSpace.s12),
                  Text(
                    "Drop a plugin's .zip here, or choose a file",
                    style: OcText.body.copyWith(color: colors.text),
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: OcSpace.s12),
                  OcButton(
                    label: 'Choose a file',
                    icon: OcIcons.folder,
                    onPressed: _chooseFile,
                  ),
                ],
              ),
            ),
          ),

          // Waiting reviews from dev mode pushes (§17.1)
          if (devReviews.isNotEmpty) ...[
            const SizedBox(height: OcSpace.s16),
            for (final review in devReviews)
              Container(
                margin: const EdgeInsets.only(bottom: OcSpace.s8),
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s16,
                  vertical: OcSpace.s12,
                ),
                decoration: BoxDecoration(
                  color: colors.surface,
                  borderRadius: BorderRadius.circular(OcRadius.section),
                  border: Border.all(color: colors.border),
                ),
                child: Row(
                  children: [
                    Icon(
                      OcIcons.extension,
                      size: 20,
                      color: colors.textSecondary,
                    ),
                    const SizedBox(width: OcSpace.s12),
                    Expanded(
                      child: Text(
                        '${review.plugin.name} wants to update from dev mode',
                        style: OcText.body.copyWith(color: colors.text),
                      ),
                    ),
                    OcButton(
                      label: 'Review',
                      dense: true,
                      onPressed: () => showReviewDialog(
                        context,
                        serverKey: widget.serverKey,
                        review: review,
                      ),
                    ),
                  ],
                ),
              ),
          ],

          const SizedBox(height: OcSpace.s24),

          // Plugins list
          if (plugins.isEmpty)
            Container(
              padding: const EdgeInsets.all(OcSpace.s32),
              alignment: Alignment.center,
              child: Text(
                'No plugins yet.',
                style: OcText.body.copyWith(color: colors.textMuted),
              ),
            )
          else
            SettingsSection(
              title: 'Installed plugins',
              children: [
                for (final plugin in plugins)
                  _PluginRow(
                    plugin: plugin,
                    onToggle: (on) => _toggleRunning(plugin, on),
                  ),
              ],
            ),
        ],
      ),
    );
  }
}

class _PluginRow extends StatelessWidget {
  const _PluginRow({
    required this.plugin,
    required this.onToggle,
  });

  final Plugin plugin;
  final ValueChanged<bool> onToggle;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final isRunningOrStarting =
        plugin.state == PluginState.running ||
        plugin.state == PluginState.starting;

    final cpuPercent = (plugin.cpuPermille / 10).toStringAsFixed(1);
    final memoryMb = (plugin.memoryBytes / (1024 * 1024)).toStringAsFixed(1);

    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s12,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Container(
            width: 36,
            height: 36,
            decoration: BoxDecoration(
              color: colors.hover,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: colors.border),
            ),
            alignment: Alignment.center,
            child: Icon(
              OcIcons.extension,
              size: 20,
              color: colors.textSecondary,
            ),
          ),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        plugin.name,
                        style: OcText.body.copyWith(
                          color: colors.text,
                          fontWeight: FontWeight.w600,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    const SizedBox(width: OcSpace.s8),
                    Text(
                      plugin.version,
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                    const SizedBox(width: OcSpace.s8),
                    Text(
                      plugin.author,
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    PluginSignatureBadge(
                      signature: plugin.signature,
                      githubCheck: plugin.githubCheck,
                    ),
                    const SizedBox(width: 6),
                    PluginStatusBadge(state: plugin.state),
                    if (plugin.state == PluginState.crashed &&
                        plugin.statusReason.isNotEmpty) ...[
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          plugin.statusReason,
                          style: OcText.small.copyWith(
                            color: colors.textSecondary,
                          ),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(width: OcSpace.s16),
          // CPU and memory metrics
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                '$cpuPercent%',
                style: OcText.mono.copyWith(
                  color: colors.text,
                  fontSize: 12,
                ),
              ),
              const SizedBox(height: 2),
              Text(
                '$memoryMb MB',
                style: OcText.mono.copyWith(
                  color: colors.textMuted,
                  fontSize: 11,
                ),
              ),
            ],
          ),
          const SizedBox(width: OcSpace.s16),
          OcSwitch(
            value: isRunningOrStarting,
            onChanged: onToggle,
          ),
        ],
      ),
    );
  }
}
