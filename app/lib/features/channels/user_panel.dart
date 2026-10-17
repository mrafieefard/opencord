import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/settings/user_settings.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

/// The user panel (§4.2): who you are, how you appear, and mute, deafen and
/// settings one click away.
class UserPanel extends ConsumerWidget {
  const UserPanel({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    final self = server == null
        ? null
        : ref.watch(serverProvider(server).select((s) => s.data?.selfMember));
    final identity = ref.watch(localIdentityProvider);
    final name = self?.user.displayName ?? identity?.displayName ?? 'You';
    final presence = ref.watch(selfPresenceProvider);
    final voice = ref.watch(voiceSessionProvider);
    final session = ref.read(voiceSessionProvider.notifier);
    return Container(
      // As tall as the composer, so the two top lines meet.
      height: math.max(
        ref.watch(restingComposerHeightProvider) ?? OcSize.userPanel,
        OcSize.userPanelLeast,
      ),
      padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
      decoration: BoxDecoration(
        color: colors.sidebar,
        border: Border(top: BorderSide(color: colors.border)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Builder(
              builder: (context) => Hoverable(
                onTap: () => _choosePresence(context, ref, presence),
                semanticLabel: '$name, ${presence.label}. Change status',
                builder: (context, state) => AnimatedContainer(
                  duration: OcMotion.of(context).hover,
                  padding: const EdgeInsets.all(OcSpace.s4),
                  decoration: BoxDecoration(
                    color: state.active ? colors.hover : null,
                    borderRadius: BorderRadius.circular(OcRadius.row),
                  ),
                  child: Row(
                    children: [
                      OcAvatar(
                        id: self?.id ?? name,
                        name: name,
                        size: 32,
                        presence: presence.shown,
                        ringColor: state.active ? colors.hover : colors.sidebar,
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            EllipsisText(
                              name,
                              style: OcText.bodyStrong.copyWith(
                                color: colors.text,
                                fontSize: 13,
                              ),
                            ),
                            Text(
                              presence.label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.meta.copyWith(
                                color: colors.textMuted,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          _SplitButton(
            side: AudioSide.input,
            button: OcIconButton(
              icon: OcIcons.mic,
              activeIcon: OcIcons.micOff,
              tooltip: voice.muted ? 'Unmute' : 'Mute',
              active: voice.muted,
              size: OcIconButtonSize.compact,
              onPressed: session.toggleMute,
            ),
          ),
          _SplitButton(
            side: AudioSide.output,
            button: OcIconButton(
              icon: OcIcons.headphones,
              activeIcon: OcIcons.headsetOff,
              tooltip: voice.deafened ? 'Undeafen' : 'Deafen',
              active: voice.deafened,
              size: OcIconButtonSize.compact,
              onPressed: session.toggleDeafen,
            ),
          ),
          OcIconButton(
            icon: OcIcons.settings,
            tooltip: 'User settings',
            size: OcIconButtonSize.compact,
            onPressed: () => showUserSettings(context),
          ),
        ],
      ),
    );
  }

  Future<void> _choosePresence(
    BuildContext context,
    WidgetRef ref,
    SelfPresence current,
  ) {
    final rect = globalRectOf(context);
    return showOcMenu(
      context: context,
      position: rect.topLeft.translate(0, -8 - 4 * 32 - 8),
      entries: [
        for (final presence in SelfPresence.values)
          OcMenuItem(
            label: presence.label,
            checked: presence == current,
            onSelected: () =>
                ref.read(selfPresenceProvider.notifier).choose(presence),
          ),
      ],
    );
  }
}

/// A button with a small chevron that opens its side's quick audio menu
/// (§4.2): the microphone's, or the speakers'.
class _SplitButton extends StatelessWidget {
  const _SplitButton({required this.side, required this.button});

  final AudioSide side;
  final Widget button;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        button,
        Builder(
          builder: (context) => Hoverable(
            onTap: () => showQuickAudioMenu(context, side),
            semanticLabel: switch (side) {
              AudioSide.input => 'Input options',
              AudioSide.output => 'Output options',
            },
            focusRadius: BorderRadius.circular(6),
            builder: (context, state) => Container(
              width: 14,
              height: 28,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: state.active ? context.oc.selected : null,
                borderRadius: BorderRadius.circular(6),
              ),
              child: Icon(
                OcIcons.arrowDropDown,
                size: 16,
                color: state.active ? context.oc.text : context.oc.textMuted,
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// Which quick audio menu: the microphone's or the speakers'.
enum AudioSide { input, output }

/// The quick audio menu (§4.2, §17.1): one side's devices and volume, and
/// for the microphone its input mode, without opening settings.
Future<void> showQuickAudioMenu(BuildContext context, AudioSide side) {
  // Devices plugged in since voice last looked.
  ProviderScope.containerOf(
    context,
    listen: false,
  ).read(audioDeviceListProvider.notifier).refresh();
  return showPopover<void>(
    context: context,
    anchor: globalRectOf(context),
    side: PopoverSide.above,
    padding: const EdgeInsets.all(OcSpace.s12),
    builder: (context) => SizedBox(
      width: 280,
      child: switch (side) {
        AudioSide.input => const _InputMenu(),
        AudioSide.output => const _OutputMenu(),
      },
    ),
  );
}

class _InputMenu extends ConsumerWidget {
  const _InputMenu();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final audio = ref.watch(audioSettingsProvider);
    final devices = ref.watch(audioDeviceListProvider);
    final update = ref.read(audioSettingsProvider.notifier).update;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SectionLabel('Input device'),
        _DeviceList(
          choices: [
            for (final (device, label) in deviceChoices(devices.inputs))
              (
                label: label,
                chosen: audio.inputDevice == device?.id,
                choose: () => update((a) => a.withInput(device)),
              ),
          ],
        ),
        const SizedBox(height: OcSpace.s10),
        const SectionLabel('Input mode'),
        const SizedBox(height: OcSpace.s4),
        Row(
          children: [
            for (final mode in InputMode.values)
              Expanded(
                child: Padding(
                  padding: EdgeInsets.only(
                    right: mode == InputMode.values.first ? OcSpace.s6 : 0,
                  ),
                  child: _ModeButton(
                    label: mode.label,
                    active: audio.inputMode == mode,
                    onTap: () => update((a) => a.copyWith(inputMode: mode)),
                  ),
                ),
              ),
          ],
        ),
        const SizedBox(height: OcSpace.s10),
        _Volume(
          label: 'Input volume',
          value: audio.inputVolume,
          onChanged: (value) => update((a) => a.copyWith(inputVolume: value)),
        ),
        const InputLevelMeter(),
      ],
    );
  }
}

class _OutputMenu extends ConsumerWidget {
  const _OutputMenu();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final audio = ref.watch(audioSettingsProvider);
    final devices = ref.watch(audioDeviceListProvider);
    final update = ref.read(audioSettingsProvider.notifier).update;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SectionLabel('Output device'),
        _DeviceList(
          choices: [
            for (final (device, label) in deviceChoices(devices.outputs))
              (
                label: label,
                chosen: audio.outputDevice == device?.id,
                choose: () => update((a) => a.withOutput(device)),
              ),
          ],
        ),
        const SizedBox(height: OcSpace.s10),
        _Volume(
          label: 'Output volume',
          value: audio.outputVolume,
          onChanged: (value) => update((a) => a.copyWith(outputVolume: value)),
        ),
      ],
    );
  }
}

/// A device list four rows tall at most; a system with more scrolls it.
class _DeviceList extends StatefulWidget {
  const _DeviceList({required this.choices});

  final List<({String label, bool chosen, VoidCallback choose})> choices;

  static const rowHeight = 30.0;
  static const rowsShown = 4;

  @override
  State<_DeviceList> createState() => _DeviceListState();
}

class _DeviceListState extends State<_DeviceList> {
  final _scroll = ScrollController();

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final choices = widget.choices;
    return SizedBox(
      height:
          _DeviceList.rowHeight *
          choices.length.clamp(1, _DeviceList.rowsShown),
      child: Scrollbar(
        controller: _scroll,
        thumbVisibility: choices.length > _DeviceList.rowsShown,
        child: ListView(
          controller: _scroll,
          padding: EdgeInsets.zero,
          itemExtent: _DeviceList.rowHeight,
          children: [
            for (final choice in choices)
              _Choice(
                label: choice.label,
                chosen: choice.chosen,
                onTap: choice.choose,
              ),
          ],
        ),
      ),
    );
  }
}

class _Choice extends StatelessWidget {
  const _Choice({
    required this.label,
    required this.chosen,
    required this.onTap,
  });

  final String label;
  final bool chosen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: label,
      selected: chosen,
      focusRadius: BorderRadius.circular(6),
      builder: (context, state) => Container(
        height: 30,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        decoration: BoxDecoration(
          color: state.active ? colors.selected : null,
          borderRadius: BorderRadius.circular(6),
        ),
        child: Row(
          children: [
            Expanded(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.small.copyWith(
                  color: chosen ? colors.text : colors.textSecondary,
                ),
              ),
            ),
            if (chosen)
              Icon(OcIcons.check, size: OcSize.iconInline, color: colors.text),
          ],
        ),
      ),
    );
  }
}

class _ModeButton extends StatelessWidget {
  const _ModeButton({
    required this.label,
    required this.active,
    required this.onTap,
  });

  final String label;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: label,
      selected: active,
      focusRadius: BorderRadius.circular(8),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 30,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: active
              ? colors.accent
              : state.active
              ? colors.selected
              : colors.hover,
          borderRadius: BorderRadius.circular(8),
        ),
        child: Text(
          label,
          style: OcText.small.copyWith(
            fontWeight: FontWeight.w600,
            color: active ? colors.onAccent : colors.text,
          ),
        ),
      ),
    );
  }
}

class _Volume extends StatelessWidget {
  const _Volume({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final int value;
  final ValueChanged<int> onChanged;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(child: SectionLabel(label)),
            Text(
              '$value%',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ],
        ),
        SliderTheme(
          data: SliderTheme.of(context).copyWith(
            trackHeight: 3,
            thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 7),
            overlayShape: const RoundSliderOverlayShape(overlayRadius: 14),
          ),
          child: Slider(
            value: value.toDouble(),
            max: AudioSettings.maxVolume.toDouble(),
            divisions: AudioSettings.maxVolume,
            semanticFormatterCallback: (value) => '${value.round()}%',
            onChanged: (value) => onChanged(value.round()),
          ),
        ),
      ],
    );
  }
}
