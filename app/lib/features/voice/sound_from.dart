import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

/// A window share's "Sound from" (Phase 2 plan §10.4): the desktop's
/// picker says nothing of which app owns the window, so this asks. The
/// app chosen before is chosen again if it is playing; else the only one
/// playing; else No sound, so no app is shared by surprise. Choosing
/// applies at once.
Future<void> chooseSoundFrom(BuildContext context) async {
  final container = ProviderScope.containerOf(context, listen: false);
  final repository = container.read(repositoryProvider);
  final List<ScreenAudioApp> playing;
  try {
    playing = await repository.screenAudioApps();
  } on RepoException {
    return;
  }
  if (!context.mounted) return;
  final share = container.read(ownScreenShareProvider);
  final preselected = share?.audioFrom != null
      ? share!.audioFrom
      : preselectedSoundFrom(playing, container.read(soundFromProvider));
  if (share?.audioFrom == null && preselected != null) {
    _choose(container, preselected);
  }
  await showOcDialog<void>(
    context: context,
    builder: (context) =>
        _SoundFromDialog(playing: playing, initial: preselected),
  );
}

void _choose(ProviderContainer container, String? app) {
  container.read(repositoryProvider).setScreenShareAudioFrom(app);
  if (app != null) container.read(soundFromProvider.notifier).chose(app);
}

class _SoundFromDialog extends ConsumerStatefulWidget {
  const _SoundFromDialog({required this.playing, required this.initial});

  final List<ScreenAudioApp> playing;
  final String? initial;

  @override
  ConsumerState<_SoundFromDialog> createState() => _SoundFromDialogState();
}

class _SoundFromDialogState extends ConsumerState<_SoundFromDialog> {
  late String? _chosen = widget.initial;

  void _pick(String? app) {
    setState(() => _chosen = app);
    _choose(ProviderScope.containerOf(context, listen: false), app);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: 'Sound from',
      width: 400,
      actions: [
        OcButton.primary(
          label: 'Done',
          onPressed: () => Navigator.pop(context),
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            'Which app is in the window you share? Only its sound goes.',
            style: OcText.body.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s12),
          _Choice(
            label: 'No sound',
            icon: OcIcons.volumeOff,
            chosen: _chosen == null,
            onTap: () => _pick(null),
          ),
          for (final app in widget.playing)
            _Choice(
              label: app.name,
              icon: OcIcons.volumeUp,
              chosen: _chosen == app.name,
              onTap: () => _pick(app.name),
            ),
          if (widget.playing.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: OcSpace.s8),
              child: Text(
                'No app is playing sound now. Start it, then choose it '
                'from the screen share menu.',
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ),
        ],
      ),
    );
  }
}

class _Choice extends StatelessWidget {
  const _Choice({
    required this.label,
    required this.icon,
    required this.chosen,
    required this.onTap,
  });

  final String label;
  final IconData icon;
  final bool chosen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Semantics(
      selected: chosen,
      inMutuallyExclusiveGroup: true,
      child: Hoverable(
        onTap: onTap,
        semanticLabel: label,
        focusRadius: BorderRadius.circular(OcRadius.row),
        builder: (context, state) => Container(
          height: 40,
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s10),
          decoration: BoxDecoration(
            color: chosen
                ? colors.selected
                : state.active
                ? colors.hover
                : null,
            borderRadius: BorderRadius.circular(OcRadius.row),
          ),
          child: Row(
            children: [
              Icon(icon, size: 18, color: colors.textSecondary),
              const SizedBox(width: OcSpace.s10),
              Expanded(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.body.copyWith(
                    color: colors.text,
                    fontWeight: chosen ? FontWeight.w600 : FontWeight.w400,
                  ),
                ),
              ),
              if (chosen) Icon(OcIcons.check, size: 18, color: colors.text),
            ],
          ),
        ),
      ),
    );
  }
}
