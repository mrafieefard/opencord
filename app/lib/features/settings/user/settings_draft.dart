import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/music_session_state.dart';
import 'package:opencord/core/providers/voice_cues.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/window/window_startup.dart';

/// Client settings changed in the settings dialog and not saved yet: as in
/// Discord, they wait for the save bar. Null: unchanged.
@immutable
class ClientSettingsDraft {
  const ClientSettingsDraft({
    this.app,
    this.audio,
    this.music,
    this.soundboard,
    this.cuesOff,
    this.keybinds,
  });

  final AppSettings? app;
  final AudioSettings? audio;
  final MusicListening? music;
  final SoundboardPrefs? soundboard;
  final Set<CueGroup>? cuesOff;
  final Keybinds? keybinds;

  ClientSettingsDraft copyWith({
    AppSettings? app,
    AudioSettings? audio,
    MusicListening? music,
    SoundboardPrefs? soundboard,
    Set<CueGroup>? cuesOff,
    Keybinds? keybinds,
  }) => ClientSettingsDraft(
    app: app ?? this.app,
    audio: audio ?? this.audio,
    music: music ?? this.music,
    soundboard: soundboard ?? this.soundboard,
    cuesOff: cuesOff ?? this.cuesOff,
    keybinds: keybinds ?? this.keybinds,
  );
}

class ClientSettingsDraftNotifier extends Notifier<ClientSettingsDraft> {
  @override
  ClientSettingsDraft build() => const ClientSettingsDraft();

  void updateApp(AppSettings Function(AppSettings settings) change) => state =
      state.copyWith(app: change(state.app ?? ref.read(appSettingsProvider)));

  void updateAudio(AudioSettings Function(AudioSettings audio) change) =>
      state = state.copyWith(
        audio: change(state.audio ?? ref.read(audioSettingsProvider)),
      );

  void updateMusic(MusicListening Function(MusicListening music) change) =>
      state = state.copyWith(
        music: change(state.music ?? ref.read(musicListeningProvider)),
      );

  void updateSoundboard(
    SoundboardPrefs Function(SoundboardPrefs prefs) change,
  ) => state = state.copyWith(
    soundboard: change(state.soundboard ?? ref.read(soundboardPrefsProvider)),
  );

  void setCue(CueGroup group, {required bool on}) {
    final Set<CueGroup> off = state.cuesOff ?? ref.read(cuePrefsProvider);
    state = state.copyWith(
      cuesOff: on ? ({...off}..remove(group)) : {...off, group},
    );
  }

  Keybinds get _keybinds => state.keybinds ?? ref.read(keybindsProvider);

  /// A new keybind row for [action]; its id.
  int keybindAdd(HotkeyAction action, {String keys = ''}) {
    final (keybinds, id) = _keybinds.added(action, keys: keys);
    state = state.copyWith(keybinds: keybinds);
    return id;
  }

  void keybindChange(int id, Keybind Function(Keybind keybind) edit) =>
      state = state.copyWith(keybinds: _keybinds.changed(id, edit));

  void keybindRemove(int id) =>
      state = state.copyWith(keybinds: _keybinds.removed(id));

  /// Whether a draft differs from what is saved.
  bool get dirty {
    final draft = state;
    String audioJson(AudioSettings audio) => jsonEncode(audio.toJson());
    return (draft.app != null && draft.app != ref.read(appSettingsProvider)) ||
        (draft.audio != null &&
            audioJson(draft.audio!) !=
                audioJson(ref.read(audioSettingsProvider))) ||
        (draft.music != null &&
            draft.music != ref.read(musicListeningProvider)) ||
        (draft.soundboard != null &&
            draft.soundboard != ref.read(soundboardPrefsProvider)) ||
        (draft.cuesOff != null &&
            !setEquals(draft.cuesOff, ref.read(cuePrefsProvider))) ||
        (draft.keybinds != null &&
            draft.keybinds != ref.read(keybindsProvider));
  }

  /// Saves every draft, then drops them.
  Future<void> save() async {
    final draft = state;
    if (draft.app case final app?) {
      final frameChanged =
          app.windowFrame != ref.read(appSettingsProvider).windowFrame;
      ref.read(appSettingsProvider.notifier).update((_) => app);
      if (frameChanged) {
        await ref.read(frameChoiceSaverProvider)(app.windowFrame);
      }
    }
    if (draft.audio case final audio?) {
      ref.read(audioSettingsProvider.notifier).update((_) => audio);
    }
    if (draft.music case final music?) {
      ref.read(musicListeningProvider.notifier).update((_) => music);
    }
    if (draft.soundboard case final soundboard?) {
      ref.read(soundboardPrefsProvider.notifier).update((_) => soundboard);
    }
    if (draft.cuesOff case final cuesOff?) {
      final cues = ref.read(cuePrefsProvider.notifier);
      for (final group in CueGroup.values) {
        cues.set(group, on: !cuesOff.contains(group));
      }
    }
    if (draft.keybinds case final keybinds?) {
      ref.read(keybindsProvider.notifier).replace(keybinds);
    }
    state = const ClientSettingsDraft();
  }

  void discard() => state = const ClientSettingsDraft();
}

/// Kept while a settings dialog shows client settings.
final clientSettingsDraftProvider =
    NotifierProvider.autoDispose<
      ClientSettingsDraftNotifier,
      ClientSettingsDraft
    >(ClientSettingsDraftNotifier.new);

/// What client settings pages show: the draft, else what is saved.
final shownAppSettingsProvider = Provider.autoDispose<AppSettings>(
  (ref) =>
      ref.watch(clientSettingsDraftProvider).app ??
      ref.watch(appSettingsProvider),
);

final shownAudioProvider = Provider.autoDispose<AudioSettings>(
  (ref) =>
      ref.watch(clientSettingsDraftProvider).audio ??
      ref.watch(audioSettingsProvider),
);

final shownMusicProvider = Provider.autoDispose<MusicListening>(
  (ref) =>
      ref.watch(clientSettingsDraftProvider).music ??
      ref.watch(musicListeningProvider),
);

final shownSoundboardProvider = Provider.autoDispose<SoundboardPrefs>(
  (ref) =>
      ref.watch(clientSettingsDraftProvider).soundboard ??
      ref.watch(soundboardPrefsProvider),
);

final shownCuesOffProvider = Provider.autoDispose<Set<CueGroup>>(
  (ref) =>
      ref.watch(clientSettingsDraftProvider).cuesOff ??
      ref.watch(cuePrefsProvider),
);

final shownKeybindsProvider = Provider.autoDispose<Keybinds>(
  (ref) =>
      ref.watch(clientSettingsDraftProvider).keybinds ??
      ref.watch(keybindsProvider),
);

/// Hands the client settings' drafts to the dialog's save bar, for as long
/// as the dialog is open.
class ClientSettingsBinding extends ConsumerStatefulWidget {
  const ClientSettingsBinding({super.key});

  @override
  ConsumerState<ClientSettingsBinding> createState() =>
      _ClientSettingsBindingState();
}

class _ClientSettingsBindingState extends ConsumerState<ClientSettingsBinding> {
  SettingsChanges? _changes;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final notifier = ref.read(clientSettingsDraftProvider.notifier);
    _changes = SettingsScope.changesOf(context)
      ?..register(
        this,
        PendingChanges(
          dirty: () => notifier.dirty,
          save: notifier.save,
          discard: notifier.discard,
        ),
      );
  }

  @override
  void dispose() {
    _changes?.unregister(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(clientSettingsDraftProvider, (_, _) => _changes?.touch());
    return const SizedBox.shrink();
  }
}
