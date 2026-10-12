import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Opens the soundboard (Phase 2 plan §11.7) next to [anchor], a global
/// rectangle, for this device's voice channel.
Future<void> showSoundboardPicker(
  BuildContext context, {
  required Rect anchor,
  PopoverSide side = PopoverSide.above,
}) => showPopover<void>(
  context: context,
  anchor: anchor,
  side: side,
  padding: EdgeInsets.zero,
  builder: (_) => const SoundboardPicker(),
);

/// Plays a sound for everyone in the call; why not shows as a toast.
Future<void> playSound(
  BuildContext context,
  WidgetRef ref,
  PickerSound sound,
) async {
  final voice = ref.read(voiceSessionProvider);
  final (server, channel) = (voice.serverKey, voice.channelId);
  if (server == null || channel == null) return;
  try {
    await ref
        .read(repositoryProvider)
        .playSound(server, channel, sound.ref, from: sound.from);
    ref
        .read(lastSoundProvider.notifier)
        .played(server, ref.read(clockProvider)());
  } on RepoException catch (error) {
    if (!context.mounted) return;
    showOcToast(context, switch (error.kind) {
      RepoErrorKind.soundCooldown => 'Wait a moment to play another sound.',
      RepoErrorKind.forbidden => "You can't play that sound here.",
      _ => error.message,
    });
  }
}

/// A sound as the picker shows it: this server's, another server's
/// (played here as an external sound), or a default one.
@immutable
class PickerSound {
  const PickerSound({
    required this.ref,
    required this.name,
    required this.favorite,
    this.emoji,
    this.from,
    this.previewServer,
  });

  /// What this server is asked to play.
  final String ref;
  final String name;
  final String? emoji;

  /// Its key among the favourites.
  final String favorite;

  /// Another server's sound, and that server.
  final ({String serverKey, Sound sound})? from;

  /// Where a preview fetches it from.
  final String? previewServer;
}

class SoundboardPicker extends ConsumerStatefulWidget {
  const SoundboardPicker({super.key});

  @override
  ConsumerState<SoundboardPicker> createState() => _SoundboardPickerState();
}

class _SoundboardPickerState extends ConsumerState<SoundboardPicker> {
  final _search = TextEditingController();
  var _query = '';

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  bool _matches(String name) =>
      _query.isEmpty || name.toLowerCase().contains(_query.toLowerCase());

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final voice = ref.watch(voiceSessionProvider);
    final server = voice.serverKey;
    final channel = voice.channelId;
    if (server == null || channel == null) {
      return const SizedBox(
        width: 360,
        child: Padding(
          padding: EdgeInsets.all(OcSpace.s16),
          child: Text('Join a voice channel to play sounds.'),
        ),
      );
    }
    final data = ref.watch(serverProvider(server)).data;
    final settings = data?.voiceSettings;
    final held = data?.permissionsIn(channel) ?? Permissions.none;
    final self = ref.watch(
      voiceProvider(server).select(
        (voice) =>
            voice[channel]?.where((p) => p.userId == data?.self.id).firstOrNull,
      ),
    );
    final prefs = ref.watch(soundboardPrefsProvider);
    final repository = ref.read(repositoryProvider);

    // Why nothing can be played here now, if so.
    final blocked = switch (settings) {
      _ when settings?.soundboardEnabled == false =>
        'The soundboard is turned off on this server',
      _ when voice.deafened || (self?.serverDeafened ?? false) =>
        "You can't play sounds while deafened",
      _ when self?.serverMuted ?? false =>
        "You can't play sounds while server muted",
      _ when self?.suppressed ?? false => "You can't speak in this channel",
      _ when !held.has(Permissions.useSoundboard) =>
        'You need Use Soundboard here',
      _ => null,
    };
    final externalBlocked = settings?.allowExternalSounds == false
        ? "This server doesn't allow sounds from other servers"
        : !held.has(Permissions.useExternalSounds)
        ? 'You need Use External Sounds here'
        : null;
    final defaultBlocked = settings?.allowDefaultSounds == false
        ? 'This server turned off default sounds'
        : null;

    final here = [
      for (final sound in ref.watch(soundsProvider(server)).values)
        if (_matches(sound.name))
          PickerSound(
            ref: sound.ref,
            name: sound.name,
            emoji: sound.emoji,
            favorite: favoriteKey(sound.ref, serverKey: server),
            previewServer: server,
          ),
    ];
    final elsewhere = [
      for (final other in ref.watch(serverListProvider))
        if (other.key != server)
          (
            name: other.name,
            sounds: [
              for (final sound in ref.watch(soundsProvider(other.key)).values)
                if (_matches(sound.name))
                  PickerSound(
                    ref: sound.externalRef,
                    name: sound.name,
                    emoji: sound.emoji,
                    favorite: favoriteKey(sound.ref, serverKey: other.key),
                    from: (serverKey: other.key, sound: sound),
                    previewServer: other.key,
                  ),
            ],
          ),
    ];
    final defaults = [
      for (final sound in repository.defaultSounds)
        if (_matches(sound.name))
          PickerSound(
            ref: sound.ref,
            name: sound.name,
            emoji: sound.emoji,
            favorite: favoriteKey(sound.ref),
            previewServer: server,
          ),
    ];
    final all = [
      ...here,
      for (final group in elsewhere) ...group.sounds,
      ...defaults,
    ];
    final favorites = [
      for (final sound in all)
        if (prefs.favorites.contains(sound.favorite)) sound,
    ];

    String? reason(PickerSound sound) =>
        blocked ??
        (sound.from != null
            ? externalBlocked
            : sound.ref.startsWith('default:')
            ? defaultBlocked
            : null);

    Widget section(String label, List<PickerSound> sounds) => _Section(
      label: label,
      sounds: sounds,
      reason: reason,
      favorites: prefs.favorites,
    );

    return ConstrainedBox(
      constraints: const BoxConstraints.tightFor(
        width: 360,
      ).copyWith(maxHeight: 440),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          _CooldownBar(
            server: server,
            cooldown: settings?.soundCooldown ?? Duration.zero,
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(
              OcSpace.s12,
              OcSpace.s12,
              OcSpace.s12,
              OcSpace.s8,
            ),
            child: OcTextField(
              controller: _search,
              autofocus: true,
              hint: 'Search sounds',
              prefixIcon: OcIcons.search,
              semanticLabel: 'Search sounds',
              onChanged: (text) => setState(() => _query = text.trim()),
            ),
          ),
          if (blocked != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
              child: Text(
                blocked,
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ),
          Flexible(
            child: ListView(
              shrinkWrap: true,
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s12,
                OcSpace.s4,
                OcSpace.s12,
                OcSpace.s12,
              ),
              children: [
                if (favorites.isNotEmpty) section('Favorites', favorites),
                if (here.isNotEmpty) section('This server', here),
                for (final group in elsewhere)
                  if (group.sounds.isNotEmpty)
                    section(group.name, group.sounds),
                if (defaults.isNotEmpty) section('Default', defaults),
                if (all.isEmpty)
                  Padding(
                    padding: const EdgeInsets.all(OcSpace.s16),
                    child: Text(
                      _query.isEmpty ? 'No sounds yet.' : 'No sounds match.',
                      textAlign: TextAlign.center,
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({
    required this.label,
    required this.sounds,
    required this.reason,
    required this.favorites,
  });

  final String label;
  final List<PickerSound> sounds;
  final String? Function(PickerSound sound) reason;
  final Set<String> favorites;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: OcSpace.s8, bottom: OcSpace.s6),
          child: Text(
            label.toUpperCase(),
            style: OcText.label.copyWith(color: colors.textMuted),
          ),
        ),
        LayoutBuilder(
          builder: (context, constraints) {
            final width = (constraints.maxWidth - 2 * OcSpace.s6) / 3;
            return Wrap(
              spacing: OcSpace.s6,
              runSpacing: OcSpace.s6,
              children: [
                for (final sound in sounds)
                  SizedBox(
                    width: width,
                    child: _SoundTile(
                      sound: sound,
                      reason: reason(sound),
                      favorite: favorites.contains(sound.favorite),
                    ),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _SoundTile extends ConsumerWidget {
  const _SoundTile({
    required this.sound,
    required this.reason,
    required this.favorite,
  });

  final PickerSound sound;

  /// Why it cannot be played here; dimmed with this as its tooltip.
  final String? reason;
  final bool favorite;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final cooling = ref.watch(_coolingProvider);
    final reason = this.reason ?? (cooling ? 'Cooling down' : null);
    final tile = Hoverable(
      onTap: reason == null ? () => playSound(context, ref, sound) : null,
      semanticLabel: reason == null
          ? 'Play ${sound.name}'
          : '${sound.name}, $reason',
      focusRadius: BorderRadius.circular(8),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 40,
        padding: const EdgeInsets.only(left: OcSpace.s8, right: 2),
        decoration: BoxDecoration(
          color: state.active && reason == null
              ? colors.selected
              : colors.hover,
          borderRadius: BorderRadius.circular(8),
        ),
        child: Row(
          children: [
            Opacity(
              opacity: reason == null ? 1 : 0.4,
              child: switch (sound.emoji) {
                final emoji? => Text(
                  emoji,
                  style: const TextStyle(fontSize: 16),
                ),
                null => Icon(
                  OcIcons.musicNote,
                  size: OcSize.iconInline,
                  color: colors.textSecondary,
                ),
              },
            ),
            const SizedBox(width: OcSpace.s6),
            Expanded(
              child: EllipsisText(
                sound.name,
                style: OcText.small.copyWith(
                  color: reason == null ? colors.text : colors.textMuted,
                ),
              ),
            ),
            if (state.hovered || favorite) ...[
              if (state.hovered)
                OcIconButton(
                  icon: OcIcons.playArrow,
                  tooltip: 'Preview',
                  size: OcIconButtonSize.compact,
                  onPressed: () => _preview(context, ref),
                ),
              OcIconButton(
                icon: OcIcons.star,
                tooltip: favorite
                    ? 'Remove from favorites'
                    : 'Add to favorites',
                size: OcIconButtonSize.compact,
                active: favorite,
                onPressed: () => ref
                    .read(soundboardPrefsProvider.notifier)
                    .toggleFavorite(sound.favorite),
              ),
            ],
          ],
        ),
      ),
    );
    return reason == null
        ? tile
        : Tooltip(message: reason, excludeFromSemantics: true, child: tile);
  }

  Future<void> _preview(BuildContext context, WidgetRef ref) async {
    final server = sound.previewServer;
    if (server == null) return;
    // Another server's sound previews from where it lives.
    final soundRef = sound.from?.sound.ref ?? sound.ref;
    try {
      await ref.read(repositoryProvider).previewSound(server, soundRef);
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }
}

/// Whether this device's cooldown is running on its voice server.
final _coolingProvider = Provider.autoDispose<bool>((ref) {
  final server = ref.watch(voiceSessionProvider.select((v) => v.serverKey));
  if (server == null) return false;
  final last = ref.watch(lastSoundProvider.select((last) => last[server]));
  final cooldown =
      ref.watch(
        serverProvider(
          server,
        ).select((s) => s.data?.voiceSettings.soundCooldown),
      ) ??
      Duration.zero;
  if (last == null) return false;
  final ready = last.add(cooldown);
  final now = ref.read(clockProvider)();
  if (!now.isBefore(ready)) return false;
  // Ready again when the cooldown runs out.
  final timer = Timer(ready.difference(now), ref.invalidateSelf);
  ref.onDispose(timer.cancel);
  return true;
});

/// A thin bar along the top that empties as the cooldown runs out (§11.7).
class _CooldownBar extends ConsumerStatefulWidget {
  const _CooldownBar({required this.server, required this.cooldown});

  final String server;
  final Duration cooldown;

  @override
  ConsumerState<_CooldownBar> createState() => _CooldownBarState();
}

class _CooldownBarState extends ConsumerState<_CooldownBar>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker((_) => setState(() {}));

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final last = ref.watch(
      lastSoundProvider.select((last) => last[widget.server]),
    );
    final now = ref.read(clockProvider)();
    final total = widget.cooldown.inMilliseconds;
    final left = last == null || total == 0
        ? 0.0
        : (1 - now.difference(last).inMilliseconds / total).clamp(0.0, 1.0);
    if (left > 0 && !_ticker.isActive) _ticker.start();
    if (left == 0 && _ticker.isActive) _ticker.stop();
    return SizedBox(
      height: 2,
      child: Align(
        alignment: Alignment.centerLeft,
        child: FractionallySizedBox(
          widthFactor: left,
          child: ColoredBox(color: context.oc.text),
        ),
      ),
    );
  }
}
