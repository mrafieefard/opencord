import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Every command, on or off, while the Commands page shows them.
class CommandSettingsNotifier extends AsyncNotifier<CommandList> {
  CommandSettingsNotifier(this.serverKey);

  final String serverKey;

  @override
  Future<CommandList> build() =>
      ref.read(repositoryProvider).commandSettings(serverKey);

  /// Failures reach the caller.
  Future<void> change({
    String name = '',
    bool? enabled,
    List<int>? channelIds,
    bool? resultsPublic,
  }) async {
    state = AsyncData(
      await ref
          .read(repositoryProvider)
          .updateCommandSettings(
            serverKey,
            name: name,
            enabled: enabled,
            channelIds: channelIds,
            resultsPublic: resultsPublic,
          ),
    );
  }

  Future<void> retry() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(build);
  }
}

final commandSettingsProvider = AsyncNotifierProvider.autoDispose
    .family<CommandSettingsNotifier, CommandList, String>(
      CommandSettingsNotifier.new,
    );

const _features = {'music': 'Music'};

/// Server settings → Commands (Phase 3 plan §17.4), for who manages the
/// server: each command on or off and where it works, and where results
/// go.
class ServerCommandsPage extends ConsumerWidget {
  const ServerCommandsPage({super.key, required this.serverKey});

  final String serverKey;

  Future<void> _change(
    BuildContext context,
    WidgetRef ref, {
    String name = '',
    bool? enabled,
    List<int>? channelIds,
    bool? resultsPublic,
  }) async {
    try {
      await ref
          .read(commandSettingsProvider(serverKey).notifier)
          .change(
            name: name,
            enabled: enabled,
            channelIds: channelIds,
            resultsPublic: resultsPublic,
          );
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final value = ref.watch(commandSettingsProvider(serverKey));
    switch (value) {
      case AsyncError(:final error):
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            InlineError(
              error is RepoException
                  ? error.message
                  : 'The commands could not be loaded.',
            ),
            const SizedBox(height: OcSpace.s12),
            OcButton(
              label: 'Try again',
              onPressed: () =>
                  ref.read(commandSettingsProvider(serverKey).notifier).retry(),
            ),
          ],
        );
      case AsyncData(:final value):
        final byFeature = <String, List<CommandSpec>>{};
        for (final spec in value.commands) {
          byFeature.putIfAbsent(spec.feature, () => []).add(spec);
        }
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SettingsSection(
              title: 'Command results',
              footer:
                  'Public results show in the channel as a short line by '
                  'whoever ran the command. Errors and lists like /queue are '
                  'always only for them.',
              children: [
                SettingsField(
                  title: 'Results go to',
                  child: ChoiceChips<bool>(
                    options: const [
                      (true, 'The channel'),
                      (false, 'Only the user'),
                    ],
                    value: value.resultsPublic,
                    onChanged: (public) =>
                        _change(context, ref, resultsPublic: public),
                  ),
                ),
              ],
            ),
            for (final MapEntry(key: feature, value: specs)
                in byFeature.entries) ...[
              const SizedBox(height: OcSpace.s24),
              SettingsSection(
                title: _features[feature] ?? feature,
                footer: feature == 'music'
                    ? 'Music commands show only while music is set up. '
                          '/volume and /music are the app’s own and always '
                          'work.'
                    : null,
                children: [
                  for (final spec in specs)
                    _CommandRow(
                      serverKey: serverKey,
                      spec: spec,
                      onEnabled: (on) =>
                          _change(context, ref, name: spec.name, enabled: on),
                      onChannels: (ids) => _change(
                        context,
                        ref,
                        name: spec.name,
                        channelIds: ids,
                      ),
                    ),
                ],
              ),
            ],
          ],
        );
      default:
        return Padding(
          padding: const EdgeInsets.all(OcSpace.s24),
          child: Center(child: OcSpinner(size: 20, color: colors.textMuted)),
        );
    }
  }
}

class _CommandRow extends ConsumerWidget {
  const _CommandRow({
    required this.serverKey,
    required this.spec,
    required this.onEnabled,
    required this.onChannels,
  });

  final String serverKey;
  final CommandSpec spec;
  final ValueChanged<bool> onEnabled;
  final ValueChanged<List<int>> onChannels;

  void _pickChannels(BuildContext anchor, List<Channel> channels) {
    final chosen = spec.channelIds.toSet();
    showOcMenu(
      context: anchor,
      position: globalRectOf(anchor).bottomLeft.translate(0, 4),
      entries: [
        OcMenuItem(
          label: 'All channels',
          checked: chosen.isEmpty,
          onSelected: () => onChannels(const []),
        ),
        const OcMenuDivider(),
        for (final channel in channels)
          OcMenuItem(
            label: channel.kind == ChannelKind.voice
                ? '${channel.name} (voice chat)'
                : '#${channel.name}',
            checked: chosen.contains(channel.id),
            onSelected: () => onChannels(
              chosen.contains(channel.id)
                  ? [...chosen.where((id) => id != channel.id)]
                  : [...chosen, channel.id],
            ),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final channels = ref.watch(
      serverProvider(serverKey).select(
        (state) => [
          for (final channel
              in state.data?.channels.values ?? const <Channel>[])
            if (channel.kind.isTextLike ||
                channel.kind == ChannelKind.voice && channel.textInVoice)
              channel,
        ]..sort((a, b) => a.position.compareTo(b.position)),
      ),
    );
    final names = {for (final channel in channels) channel.id: channel.name};
    final allowed = spec.channelIds.isEmpty
        ? 'All channels'
        : spec.channelIds.map((id) => names[id] ?? 'deleted').join(', ');
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s10,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '/${spec.name}',
                  style: OcText.bodyStrong.copyWith(color: colors.text),
                ),
                Text(
                  '${spec.description} · ${spec.permissionSummary}',
                  style: OcText.small.copyWith(color: colors.textSecondary),
                ),
                const SizedBox(height: OcSpace.s4),
                Builder(
                  builder: (anchor) => Hoverable(
                    onTap: () => _pickChannels(anchor, channels),
                    semanticLabel: '/${spec.name} allowed in: $allowed',
                    focusRadius: BorderRadius.circular(4),
                    builder: (context, state) => Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          OcIcons.tag,
                          size: OcSize.iconInline,
                          color: colors.textMuted,
                        ),
                        const SizedBox(width: OcSpace.s4),
                        Flexible(
                          child: Text(
                            'Allowed in: $allowed',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: OcText.small.copyWith(
                              color: state.active
                                  ? colors.text
                                  : colors.textSecondary,
                              decoration: state.hovered
                                  ? TextDecoration.underline
                                  : null,
                            ),
                          ),
                        ),
                        Icon(
                          OcIcons.expandMore,
                          size: OcSize.iconInline,
                          color: colors.textMuted,
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
          OcSwitch(
            value: spec.enabled,
            onChanged: onEnabled,
            semanticLabel: '/${spec.name}',
          ),
        ],
      ),
    );
  }
}
