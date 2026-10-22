import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// Server settings → Voice & video (Phase 2 plan §5.2): the screen share
/// cap, cameras, the voice bitrate cap and the AFK channel.
class ServerVoicePage extends ConsumerStatefulWidget {
  const ServerVoicePage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerVoicePage> createState() => _ServerVoicePageState();
}

class _ServerVoicePageState extends ConsumerState<ServerVoicePage>
    with SaveBarChanges {
  /// As saved; kept here for the save bar (see [SaveBarChanges]).
  late VoiceSettings _saved = _read();
  late VoiceSettings _draft = _saved;

  VoiceSettings _read() =>
      ref.read(serverProvider(widget.serverKey)).data?.voiceSettings ??
      const VoiceSettings();

  @override
  bool get unsavedChanges => _draft != _saved;

  void _edit(VoiceSettings Function(VoiceSettings draft) change) {
    setState(() => _draft = change(_draft));
    changesEdited();
  }

  @override
  Future<void> saveChanges() async {
    final draft = _draft;
    await ref
        .read(repositoryProvider)
        .updateVoiceSettings(widget.serverKey, draft);
    _saved = draft;
  }

  @override
  void discardChanges() {
    if (mounted) setState(() => _draft = _saved);
  }

  void _pickAfkChannel(BuildContext anchor, List<Channel> voiceChannels) {
    showOcMenu(
      context: anchor,
      position: globalRectOf(anchor).bottomLeft.translate(0, 4),
      entries: [
        OcMenuItem(
          label: 'None',
          checked: _draft.afkChannelId == null,
          onSelected: () => _edit((d) => d.copyWith(afkChannelId: () => null)),
        ),
        for (final channel in voiceChannels)
          OcMenuItem(
            label: channel.name,
            checked: channel.id == _draft.afkChannelId,
            onSelected: () =>
                _edit((d) => d.copyWith(afkChannelId: () => channel.id)),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final saved = ref.watch(
      serverProvider(
        widget.serverKey,
      ).select((state) => state.data?.voiceSettings),
    );
    if (saved != null && saved != _saved) {
      // Saved here or elsewhere: what is not being edited follows it.
      if (_draft == _saved) _draft = saved;
      _saved = saved;
    }
    final channels =
        ref.watch(
          serverProvider(
            widget.serverKey,
          ).select((state) => state.data?.channels),
        ) ??
        const <int, Channel>{};
    final voiceChannels =
        channels.values.where((c) => c.kind == ChannelKind.voice).toList()
          ..sort((a, b) => a.position.compareTo(b.position));
    final afk = channels[_draft.afkChannelId];
    final draft = _draft;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Screen share',
          footer:
              'People sharing their screen pick up to these. Higher '
              "settings use more of the server's upload.",
          children: [
            SettingsField(
              title: 'Highest resolution',
              child: ChoiceChips<ScreenShareResolution>(
                options: [
                  for (final resolution in ScreenShareResolution.values)
                    (resolution, resolution.label),
                ],
                value: draft.screenShareMaxResolution,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(screenShareMaxResolution: value)),
              ),
            ),
            SettingsField(
              title: 'Highest frame rate',
              child: ChoiceChips<int>(
                options: [
                  for (final fps in VoiceSettings.fpsChoices) (fps, '$fps fps'),
                ],
                value: draft.screenShareMaxFps,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(screenShareMaxFps: value)),
              ),
            ),
            SettingsSliderRow(
              title: 'Viewers per stream',
              value: draft.maxStreamViewers,
              min: 1,
              max: VoiceSettings.maxStreamViewersLimit,
              shown: '${draft.maxStreamViewers}',
              onChanged: (value) =>
                  _edit((d) => d.copyWith(maxStreamViewers: value)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Camera',
          children: [
            SettingsSwitchRow(
              title: 'Allow cameras',
              value: draft.cameraAllowed,
              onChanged: (value) =>
                  _edit((d) => d.copyWith(cameraAllowed: value)),
            ),
            SettingsSliderRow(
              title: 'Cameras on at once, per channel',
              value: draft.maxCameraParticipants,
              min: 1,
              max: VoiceSettings.maxCameraParticipantsLimit,
              shown: '${draft.maxCameraParticipants}',
              onChanged: (value) =>
                  _edit((d) => d.copyWith(maxCameraParticipants: value)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Voice',
          footer:
              "Caps every voice channel's bitrate. Higher sounds clearer and "
              'uses more bandwidth.',
          children: [
            SettingsSliderRow(
              title: 'Highest voice bitrate',
              value: draft.maxVoiceBitrate ~/ 1000,
              min: VoiceSettings.minVoiceBitrate ~/ 1000,
              max: VoiceSettings.maxVoiceBitrateLimit ~/ 1000,
              step: 8,
              shown: '${draft.maxVoiceBitrate ~/ 1000} kbps',
              onChanged: (value) =>
                  _edit((d) => d.copyWith(maxVoiceBitrate: value * 1000)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Idle',
          footer:
              'People who stay idle this long are moved to the AFK channel, '
              "where they can't speak.",
          children: [
            Builder(
              builder: (context) => SettingsRow(
                title: 'AFK channel',
                subtitle: afk?.name ?? 'None',
                icon: OcIcons.schedule,
                trailing: Icon(
                  OcIcons.expandMore,
                  size: 18,
                  color: colors.textMuted,
                ),
                onTap: () => _pickAfkChannel(context, voiceChannels),
              ),
            ),
            SettingsField(
              title: 'Move idle people after',
              child: ChoiceChips<Duration>(
                options: [
                  for (final timeout in VoiceSettings.afkTimeoutChoices)
                    (timeout, '${timeout.inMinutes} min'),
                ],
                value: draft.afkTimeout,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(afkTimeout: value)),
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        Text(
          'Screen share, cameras and idle moves arrive with later versions '
          'of voice; these settings are kept for them.',
          style: OcText.small.copyWith(color: colors.textMuted),
        ),
      ],
    );
  }
}
