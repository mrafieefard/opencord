import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

String _minutes(Duration duration) => switch (duration) {
  Duration.zero => 'No limit',
  final d when d.inHours >= 1 && d.inMinutes % 60 == 0 => '${d.inHours} h',
  final d when d.inMinutes >= 1 => '${d.inMinutes} min',
  final d => '${d.inSeconds} s',
};

/// Server settings → Music, the settings themselves (Phase 3 plan §16.2):
/// a draft saved together, as on the other settings pages.
class MusicSettingsSection extends ConsumerStatefulWidget {
  const MusicSettingsSection({
    super.key,
    required this.serverKey,
    required this.saved,
  });

  final String serverKey;
  final MusicSettings saved;

  @override
  ConsumerState<MusicSettingsSection> createState() =>
      _MusicSettingsSectionState();
}

class _MusicSettingsSectionState extends ConsumerState<MusicSettingsSection> {
  Map<Object, bool Function()>? _unsaved;
  late MusicSettings _draft = widget.saved;
  var _saving = false;
  String? _error;

  bool get _changed => _draft != widget.saved;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved = SettingsScope.of(context)?..[this] = () => _changed;
  }

  @override
  void didUpdateWidget(MusicSettingsSection old) {
    super.didUpdateWidget(old);
    // Saved elsewhere, and nothing changed here: show what is saved now.
    if (old.saved != widget.saved && _draft == old.saved) {
      _draft = widget.saved;
    }
  }

  @override
  void dispose() {
    _unsaved?.remove(this);
    super.dispose();
  }

  void _edit(MusicSettings Function(MusicSettings draft) change) =>
      setState(() => _draft = change(_draft));

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(musicSettingsProvider(widget.serverKey).notifier)
          .save(_draft);
      if (mounted) showOcToast(context, 'Changes saved');
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  /// Turns a source on or off. All on is kept as "every source", so new
  /// ones a node adds later are on too.
  void _toggleSource(String key, bool on, List<String> offered) {
    final current = _draft.enabledSources.isEmpty
        ? offered
        : _draft.enabledSources;
    final next = [
      for (final source in musicSources)
        if (source.key == key ? on : current.contains(source.key)) source.key,
    ];
    final everything = offered.every(next.contains);
    _edit(
      (d) => d.copyWith(
        enabledSources: everything ? const [] : next,
        defaultSource: next.contains(d.defaultSource) || next.isEmpty
            ? null
            : next.first,
      ),
    );
  }

  void _pickBridgeDomain(BuildContext anchor, List<String> domains) {
    showOcMenu(
      context: anchor,
      position: globalRectOf(anchor).bottomLeft.translate(0, 4),
      entries: [
        OcMenuItem(
          label: 'Primary domain',
          checked: _draft.bridgeDomain == 'primary',
          onSelected: () => _edit((d) => d.copyWith(bridgeDomain: 'primary')),
        ),
        for (final domain in domains)
          OcMenuItem(
            label: domain,
            checked: _draft.bridgeDomain == domain,
            onSelected: () => _edit((d) => d.copyWith(bridgeDomain: domain)),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final draft = _draft;
    final isOwner =
        ref.watch(
          serverProvider(widget.serverKey).select((s) => s.data?.isOwner),
        ) ??
        false;
    // The domains Lavalink may connect to: the owner's to see.
    final network = isOwner
        ? ref.watch(networkProvider(widget.serverKey)).value
        : null;
    final domains = [
      for (final domain in network?.domains ?? const <Domain>[])
        if (domain.active && domain.trustedCertificate) domain.hostname,
    ];
    // Sources a node offers now, and any the admin chose before.
    final offered = [
      for (final source in musicSources)
        if (draft.availableSources.contains(source.key) ||
            draft.enabledSources.contains(source.key))
          source.key,
    ];
    final allowed = [
      for (final key in offered)
        if (draft.allows(key)) key,
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!draft.supported) ...[
          const InlineError(
            "Music isn't running on this server: it needs [music] enabled "
            'in opencord.toml, and voice on this server.',
          ),
          const SizedBox(height: OcSpace.s16),
        ],
        SettingsSection(
          title: 'Music',
          children: [
            SettingsSwitchRow(
              title: 'Music enabled',
              subtitle: 'Turning it off ends the music in every channel',
              icon: OcIcons.musicNote,
              value: draft.enabled,
              onChanged: (on) => _edit((d) => d.copyWith(enabled: on)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Search',
          footer: offered.isEmpty
              ? 'Sources appear here once a Lavalink node offers them.'
              : 'Members search only the sources turned on here. You choose, '
                    'and are responsible for, the sources you enable.',
          children: [
            for (final key in offered)
              SettingsSwitchRow(
                title: musicSourceLabel(key),
                subtitle: draft.availableSources.contains(key)
                    ? null
                    : 'No node offers it right now',
                value: draft.allows(key),
                onChanged: (on) => _toggleSource(key, on, offered),
              ),
            if (allowed.isNotEmpty)
              SettingsField(
                title: 'Default source',
                child: ChoiceChips<String>(
                  options: [
                    for (final key in allowed) (key, musicSourceLabel(key)),
                  ],
                  value: allowed.contains(draft.defaultSource)
                      ? draft.defaultSource
                      : allowed.first,
                  onChanged: (key) =>
                      _edit((d) => d.copyWith(defaultSource: key)),
                ),
              ),
            SettingsSwitchRow(
              title: 'Allow links',
              subtitle: 'https links to the enabled sources',
              value: draft.allowLinks,
              onChanged: (on) => _edit((d) => d.copyWith(allowLinks: on)),
            ),
            SettingsSwitchRow(
              title: 'Allow live streams',
              value: draft.allowStreams,
              onChanged: (on) => _edit((d) => d.copyWith(allowStreams: on)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Limits',
          footer:
              'Members who manage music are not held to the track length '
              'and per-member limits.',
          children: [
            SettingsField(
              title: 'Longest track',
              child: ChoiceChips<Duration>(
                options: [
                  for (final choice in MusicSettings.maxTrackChoices)
                    (choice, _minutes(choice)),
                ],
                value: draft.maxTrack,
                onChanged: (value) => _edit((d) => d.copyWith(maxTrack: value)),
              ),
            ),
            SettingsSliderRow(
              title: 'Tracks per member in the queue',
              value: draft.maxTracksPerMember.clamp(1, 100),
              min: 1,
              max: 100,
              shown: '${draft.maxTracksPerMember}',
              onChanged: (value) =>
                  _edit((d) => d.copyWith(maxTracksPerMember: value)),
            ),
            SettingsSliderRow(
              title: 'Tracks added from one playlist',
              value: draft.maxPlaylistTracks,
              min: 10,
              max: MusicSettings.queueLimitMax,
              step: 10,
              shown: '${draft.maxPlaylistTracks}',
              onChanged: (value) =>
                  _edit((d) => d.copyWith(maxPlaylistTracks: value)),
            ),
            SettingsSliderRow(
              title: 'Queue limit',
              value: draft.queueLimit,
              min: 10,
              max: MusicSettings.queueLimitMax,
              step: 10,
              shown: '${draft.queueLimit}',
              onChanged: (value) => _edit((d) => d.copyWith(queueLimit: value)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Vote skip',
          footer:
              'Members who may not control playback can vote to skip the '
              'track that plays.',
          children: [
            SettingsSwitchRow(
              title: 'Vote skip',
              value: draft.voteSkip,
              onChanged: (on) => _edit((d) => d.copyWith(voteSkip: on)),
            ),
            if (draft.voteSkip)
              SettingsSliderRow(
                title: 'Votes needed, of those listening',
                value: draft.voteSkipPercent,
                min: 10,
                max: 100,
                step: 10,
                shown: '${draft.voteSkipPercent} %',
                onChanged: (value) =>
                    _edit((d) => d.copyWith(voteSkipPercent: value)),
              ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'When nobody listens',
          footer:
              'Music pauses as soon as everyone leaves, and the queue is '
              'kept for 30 minutes.',
          children: [
            SettingsField(
              title: 'Stop after everyone leaves',
              child: ChoiceChips<Duration>(
                options: [
                  for (final choice in MusicSettings.stopAfterEmptyChoices)
                    (choice, _minutes(choice)),
                ],
                value: draft.stopAfterEmpty,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(stopAfterEmpty: value)),
              ),
            ),
            SettingsField(
              title: 'Disconnect after the queue ends',
              child: ChoiceChips<Duration>(
                options: [
                  for (final choice in MusicSettings.disconnectAfterIdleChoices)
                    (choice, _minutes(choice)),
                ],
                value: draft.disconnectAfterIdle,
                onChanged: (value) =>
                    _edit((d) => d.copyWith(disconnectAfterIdle: value)),
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Bridge domain',
          footer:
              'The domain Lavalink connects to. It needs a trusted '
              'certificate.',
          children: [
            Builder(
              builder: (anchor) => SettingsRow(
                title: 'Lavalink connects to',
                subtitle: draft.bridgeDomain == 'primary'
                    ? 'Primary domain'
                    : draft.bridgeDomain,
                icon: OcIcons.dns,
                trailing: isOwner
                    ? Icon(
                        OcIcons.expandMore,
                        size: 18,
                        color: colors.textMuted,
                      )
                    : null,
                onTap: isOwner
                    ? () => _pickBridgeDomain(anchor, domains)
                    : null,
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
      ],
    );
  }
}
