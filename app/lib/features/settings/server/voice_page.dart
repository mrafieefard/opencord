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
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Server settings → Voice & video (Phase 2 plan §5.2): the screen share
/// cap, cameras, the voice bitrate cap and the AFK channel.
class ServerVoicePage extends ConsumerStatefulWidget {
  const ServerVoicePage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerVoicePage> createState() => _ServerVoicePageState();
}

class _ServerVoicePageState extends ConsumerState<ServerVoicePage> {
  Map<Object, bool Function()>? _unsaved;
  late VoiceSettings _draft = _saved;
  var _saving = false;
  String? _error;

  VoiceSettings get _saved =>
      ref.read(serverProvider(widget.serverKey)).data?.voiceSettings ??
      const VoiceSettings();

  bool get _changed => _draft != _saved;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved = SettingsScope.of(context)?..[this] = () => _changed;
  }

  @override
  void dispose() {
    _unsaved?.remove(this);
    super.dispose();
  }

  void _edit(VoiceSettings Function(VoiceSettings draft) change) =>
      setState(() => _draft = change(_draft));

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(repositoryProvider)
          .updateVoiceSettings(widget.serverKey, _draft);
      if (mounted) showOcToast(context, 'Changes saved');
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
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
            _Field(
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
            _Field(
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
            _Field(
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
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s12),
          InlineError(error),
        ],
        const SizedBox(height: OcSpace.s16),
        Align(
          alignment: Alignment.centerRight,
          child: OcButton.primary(
            label: 'Save changes',
            busy: _saving,
            onPressed: _changed && !_saving ? _save : null,
          ),
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

/// A titled control inside a settings section.
class _Field extends StatelessWidget {
  const _Field({required this.title, required this.child});

  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.all(OcSpace.s16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: OcText.body.copyWith(color: colors.text)),
          const SizedBox(height: OcSpace.s8),
          child,
        ],
      ),
    );
  }
}
