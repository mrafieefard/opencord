import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/emoji_picker.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/soundboard/trim_editor.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Server settings → Soundboard (Phase 2 plan §11.7): the soundboard's
/// settings (Manage server) and its sounds (Manage soundboard).
class ServerSoundboardPage extends ConsumerStatefulWidget {
  const ServerSoundboardPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerSoundboardPage> createState() =>
      _ServerSoundboardPageState();
}

class _ServerSoundboardPageState extends ConsumerState<ServerSoundboardPage>
    with SaveBarChanges {
  /// As saved; kept here for the save bar (see [SaveBarChanges]).
  late VoiceSettings _saved =
      ref.read(serverProvider(widget.serverKey)).data?.voiceSettings ??
      const VoiceSettings();
  late VoiceSettings _draft = _saved;

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

  Future<void> _upload() async {
    final sound = await uploadSoundFromFile(
      context,
      ref,
      serverKey: widget.serverKey,
    );
    if (sound != null && mounted) showOcToast(context, '${sound.name} added');
  }

  Future<void> _delete(Sound sound) async {
    final confirmed = await confirmAction(
      context,
      title: 'Delete ${sound.name}?',
      message: 'It is removed from the soundboard for everyone.',
      action: 'Delete sound',
    );
    if (!confirmed || !mounted) return;
    try {
      await ref
          .read(repositoryProvider)
          .deleteSound(widget.serverKey, sound.id);
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  Future<void> _preview(Sound sound) async {
    try {
      await ref
          .read(repositoryProvider)
          .previewSound(widget.serverKey, sound.ref);
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final data = ref.watch(serverProvider(widget.serverKey)).data;
    if (data != null && data.voiceSettings != _saved) {
      // Saved here or elsewhere: what is not being edited follows it.
      if (_draft == _saved) _draft = data.voiceSettings;
      _saved = data.voiceSettings;
    }
    final manageServer = data?.can(Permissions.manageServer) ?? false;
    final manageSounds = data?.can(Permissions.manageSoundboard) ?? false;
    final sounds = ref.watch(soundsProvider(widget.serverKey)).values.toList();
    final max = data?.voiceSettings.maxSounds ?? _saved.maxSounds;
    final draft = _draft;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (manageServer) ...[
          SettingsSection(
            title: 'Soundboard',
            footer:
                'Sounds play for everyone in a voice channel, on their own '
                'devices. Cooldowns and the limit of three at once keep it '
                'bearable.',
            children: [
              SettingsSwitchRow(
                title: 'Soundboard enabled',
                value: draft.soundboardEnabled,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(soundboardEnabled: value)),
              ),
              SettingsSwitchRow(
                title: 'Allow default sounds',
                subtitle: 'The sounds that come with Opencord.',
                value: draft.allowDefaultSounds,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(allowDefaultSounds: value)),
              ),
              SettingsSwitchRow(
                title: 'Allow sounds from other servers',
                subtitle: 'For people with Use external sounds.',
                value: draft.allowExternalSounds,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(allowExternalSounds: value)),
              ),
              SettingsSliderRow(
                title: 'Cooldown between sounds, per person',
                value: draft.soundCooldown.inSeconds,
                min: 0,
                max: VoiceSettings.maxSoundCooldown.inSeconds,
                shown: '${draft.soundCooldown.inSeconds} s',
                onChanged: (value) => _edit(
                  (d) => d.copyWith(soundCooldown: Duration(seconds: value)),
                ),
              ),
              SettingsSliderRow(
                title: 'Most sounds',
                value: draft.maxSounds,
                min: 0,
                max: VoiceSettings.maxSoundsLimit,
                shown: '${draft.maxSounds}',
                onChanged: (value) =>
                    _edit((d) => d.copyWith(maxSounds: value)),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s24),
        ],
        Row(
          children: [
            Expanded(
              child: SectionLabel(
                'Sounds',
                trailing: Text(
                  '${sounds.length} / $max',
                  style: OcText.small.copyWith(color: colors.textMuted),
                ),
              ),
            ),
            if (manageSounds) ...[
              const SizedBox(width: OcSpace.s12),
              OcButton(
                label: 'Upload sound',
                icon: OcIcons.upload,
                onPressed: sounds.length < max ? _upload : null,
              ),
            ],
          ],
        ),
        const SizedBox(height: OcSpace.s8),
        if (sounds.isEmpty)
          Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: Text(
              manageSounds
                  ? 'No sounds yet. Upload one from an audio file.'
                  : 'No sounds yet.',
              textAlign: TextAlign.center,
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          )
        else
          for (final sound in sounds)
            _SoundRow(
              sound: sound,
              uploader: switch (sound.uploaderId) {
                final id? => data?.members[id]?.displayName,
                null => null,
              },
              onPreview: () => _preview(sound),
              onEdit: manageSounds
                  ? () => showOcDialog<void>(
                      context: context,
                      builder: (_) =>
                          _EditSound(serverKey: widget.serverKey, sound: sound),
                    )
                  : null,
              onDelete: manageSounds ? () => _delete(sound) : null,
            ),
      ],
    );
  }
}

class _SoundRow extends StatelessWidget {
  const _SoundRow({
    required this.sound,
    required this.uploader,
    required this.onPreview,
    required this.onEdit,
    required this.onDelete,
  });

  final Sound sound;
  final String? uploader;
  final VoidCallback onPreview;
  final VoidCallback? onEdit;
  final VoidCallback? onDelete;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final seconds = (sound.duration.inMilliseconds / 1000).toStringAsFixed(1);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Container(
        padding: const EdgeInsets.fromLTRB(
          OcSpace.s12,
          OcSpace.s6,
          OcSpace.s6,
          OcSpace.s6,
        ),
        decoration: BoxDecoration(
          color: colors.hover,
          borderRadius: BorderRadius.circular(8),
        ),
        child: Row(
          children: [
            SizedBox(
              width: 28,
              child: switch (sound.emoji) {
                final emoji? => Text(
                  emoji,
                  style: const TextStyle(fontSize: 18),
                ),
                null => Icon(
                  OcIcons.musicNote,
                  size: OcSize.iconInline,
                  color: colors.textSecondary,
                ),
              },
            ),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  EllipsisText(sound.name, style: OcText.bodyStrong),
                  Text(
                    [
                      '$seconds s',
                      '${sound.volume} %',
                      if (uploader case final uploader?) 'by $uploader',
                    ].join(' · '),
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                ],
              ),
            ),
            OcIconButton(
              icon: OcIcons.playArrow,
              tooltip: 'Preview',
              onPressed: onPreview,
            ),
            if (onEdit != null)
              OcIconButton(
                icon: OcIcons.edit,
                tooltip: 'Edit',
                onPressed: onEdit,
              ),
            if (onDelete != null)
              OcIconButton(
                icon: OcIcons.delete,
                tooltip: 'Delete',
                onPressed: onDelete,
              ),
          ],
        ),
      ),
    );
  }
}

/// Renames a sound, changes its emoji or volume.
class _EditSound extends ConsumerStatefulWidget {
  const _EditSound({required this.serverKey, required this.sound});

  final String serverKey;
  final Sound sound;

  @override
  ConsumerState<_EditSound> createState() => _EditSoundState();
}

class _EditSoundState extends ConsumerState<_EditSound> {
  late final _name = TextEditingController(text: widget.sound.name);
  late String? _emoji = widget.sound.emoji;
  late var _volume = widget.sound.volume;
  var _saving = false;
  String? _error;

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(repositoryProvider)
          .updateSound(
            widget.serverKey,
            widget.sound.id,
            name: _name.text.trim(),
            emoji: _emoji ?? '',
            volume: _volume,
          );
      if (mounted) Navigator.pop(context);
    } on RepoException catch (error) {
      if (mounted) {
        setState(() {
          _saving = false;
          _error = error.message;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return OcDialog(
      title: 'Edit sound',
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Save',
          busy: _saving,
          onPressed: _saving ? null : _save,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const SectionLabel('Name'),
          const SizedBox(height: OcSpace.s6),
          Row(
            children: [
              Builder(
                builder: (anchor) => OcIconButton(
                  icon: OcIcons.mood,
                  tooltip: _emoji == null ? 'Choose an emoji' : 'Change emoji',
                  onPressed: () async {
                    final emoji = await showEmojiPicker(
                      anchor,
                      anchor: globalRectOf(anchor),
                      side: PopoverSide.below,
                      alignEnd: false,
                    );
                    if (emoji != null && mounted) {
                      setState(() => _emoji = emoji);
                    }
                  },
                ),
              ),
              if (_emoji case final emoji?) ...[
                Text(emoji, style: const TextStyle(fontSize: 20)),
                OcIconButton(
                  icon: OcIcons.close,
                  tooltip: 'Remove emoji',
                  size: OcIconButtonSize.compact,
                  onPressed: () => setState(() => _emoji = null),
                ),
              ],
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: OcTextField(
                  controller: _name,
                  maxLength: 32,
                  semanticLabel: 'Sound name',
                ),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s8),
          SettingsSliderRow(
            title: 'Volume',
            value: _volume,
            min: 0,
            max: 100,
            shown: '$_volume %',
            onChanged: (value) => setState(() => _volume = value),
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s8),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}
