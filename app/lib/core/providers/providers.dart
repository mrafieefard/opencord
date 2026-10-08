import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'package:clock/clock.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/hotkeys.dart';

import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/activity_state.dart';
import 'package:opencord/core/providers/channel_audience.dart';
import 'package:opencord/core/providers/messages_state.dart';
import 'package:opencord/core/providers/pins_state.dart';
import 'package:opencord/core/providers/presence_state.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/core/providers/typing_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/core/settings/key_value_store.dart';

export 'package:opencord/core/providers/activity_state.dart';
export 'package:opencord/core/providers/channel_audience.dart';
export 'package:opencord/core/providers/messages_state.dart';
export 'package:opencord/core/providers/pins_state.dart';
export 'package:opencord/core/providers/presence_state.dart';
export 'package:opencord/core/providers/server_state.dart';
export 'package:opencord/core/providers/typing_state.dart';

/// A channel on a server: the key of per-channel providers.
typedef ChannelRef = ({String server, int channel});

/// The current time; overridden in tests.
final clockProvider = Provider<DateTime Function()>((ref) => clock.now);

/// Routes every repository event to the providers it concerns, and the
/// audio choices to voice media. Read once at startup.
final eventPumpProvider = Provider<void>((ref) {
  final repository = ref.watch(repositoryProvider);
  final subscription = repository.events.listen((event) => _route(ref, event));
  ref.onDispose(subscription.cancel);
  ref.listen(
    audioSettingsProvider,
    (_, audio) => repository.applyAudio(audio.config),
    fireImmediately: true,
  );
  _chooseNoiseSuppression(ref, repository);
  ref.listen(
    hotkeyBindingsProvider,
    (_, hotkeys) => _applyHotkeys(ref, repository, hotkeys),
    fireImmediately: true,
  );
  final inApp = _InAppHotkeys(ref);
  HardwareKeyboard.instance.addHandler(inApp.handle);
  ref.onDispose(() => HardwareKeyboard.instance.removeHandler(inApp.handle));
});

/// Binds the hotkeys system-wide where the system allows (Phase 2 plan
/// §7.13); otherwise they act only while Opencord is focused.
void _applyHotkeys(
  Ref ref,
  OpencordRepository repository,
  HotkeyBindings hotkeys,
) {
  void answered(HotkeySupport support) {
    if (ref.mounted) ref.read(hotkeySupportProvider.notifier).set(support);
  }

  unawaited(
    repository
        .setHotkeys(hotkeys.bindings)
        .then(
          answered,
          onError: (Object error) =>
              answered(HotkeySupport.focusedOnly(error.toString())),
        ),
  );
}

/// The hotkeys, while Opencord is focused and the system does not bind
/// them itself. Keys still reach everything else.
class _InAppHotkeys {
  _InAppHotkeys(this.ref);

  final Ref ref;
  final _held = <HotkeyAction>{};

  bool handle(KeyEvent event) {
    if (ref.read(hotkeySupportProvider) is! HotkeysFocusedOnly) return false;
    final bindings = ref.read(hotkeyBindingsProvider).bindings;
    for (final MapEntry(key: action, value: text) in bindings.entries) {
      final accelerator = Accelerator.parse(text);
      if (accelerator == null || event.logicalKey != accelerator.key) continue;
      switch (event) {
        case KeyDownEvent()
            when accelerator.modifiersHeld(HardwareKeyboard.instance) &&
                _held.add(action):
          _act(action, pressed: true);
        case KeyUpEvent() when _held.remove(action):
          _act(action, pressed: false);
        default:
          break;
      }
    }
    return false;
  }

  void _act(HotkeyAction action, {required bool pressed}) {
    final repository = ref.read(repositoryProvider);
    switch (action) {
      case HotkeyAction.pushToTalk:
        repository.setPushToTalk(pressed);
      case HotkeyAction.prioritySpeaker:
        repository.setPrioritySpeaker(pressed);
      case HotkeyAction.toggleMute || HotkeyAction.toggleDeafen when pressed:
        _toggle(ref, action);
      case HotkeyAction.toggleMute || HotkeyAction.toggleDeafen:
        break;
    }
  }
}

void _toggle(Ref ref, HotkeyAction action) {
  final session = ref.read(voiceSessionProvider.notifier);
  unawaited(
    action == HotkeyAction.toggleDeafen
        ? session.toggleDeafen()
        : session.toggleMute(),
  );
}

/// On the first run, High noise suppression when this computer runs it
/// easily, Standard otherwise (Phase 2 plan §7.3). A choice made meanwhile
/// stands.
void _chooseNoiseSuppression(Ref ref, OpencordRepository repository) {
  if (ref.read(audioSettingsProvider).noiseSuppression != null) return;
  unawaited(
    repository.recommendedNoiseSuppression().then(
      (mode) {
        if (!ref.mounted) return;
        ref
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio.noiseSuppression == null
                  ? audio.copyWith(noiseSuppression: mode)
                  : audio,
            );
      },
      // Standard meanwhile; the next start asks again.
      onError: (Object _) {},
    ),
  );
}

/// "You're muted" (Phase 2 plan §12): only while this device muted itself,
/// so Unmute can help; not while deafened, server-muted or unable to speak.
void _remindMuted(Ref ref) {
  final session = ref.read(voiceSessionProvider);
  final server = session.serverKey;
  final channel = session.channelId;
  if (server == null || channel == null) return;
  if (!session.muted || session.deafened) return;
  final self = ref.read(serverProvider(server)).data?.self.id;
  final me = ref
      .read(voiceProvider(server))[channel]
      ?.where((participant) => participant.userId == self)
      .firstOrNull;
  if (me != null && (me.serverMuted || me.suppressed)) return;
  ref
      .read(audioNoticeProvider.notifier)
      .show(
        "You're muted",
        actionLabel: 'Unmute',
        onAction: () => ref.read(voiceSessionProvider.notifier).toggleMute(),
      );
}

void _route(Ref ref, RepoEvent event) {
  switch (event) {
    case ServersChanged():
      ref.read(serverListProvider.notifier).refresh();
      return;
    case AudioDevicesChanged(:final devices):
      ref.read(audioDeviceListProvider.notifier).set(devices);
      return;
    case AudioDeviceFellBack(:final output, :final device):
      final which = output ? 'Speaker' : 'Microphone';
      ref
          .read(audioNoticeProvider.notifier)
          .show('$which unavailable. Using $device.');
      return;
    case AudioDeviceFailed(:final output):
      final which = output ? 'speaker' : 'microphone';
      ref.read(audioNoticeProvider.notifier).show('No $which could be opened.');
      return;
    case InputLevelChanged(:final dbfs):
      ref.read(inputLevelProvider.notifier).set(dbfs);
      return;
    case SpokeWhileMuted():
      _remindMuted(ref);
      return;
    case HotkeyPressed(:final action):
      _toggle(ref, action);
      return;
    case NoiseSuppressionFellBack():
      ref
          .read(audioSettingsProvider.notifier)
          .update(
            (audio) =>
                audio.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      ref
          .read(audioNoticeProvider.notifier)
          .show('High noise suppression was too slow here. Using Standard.');
      return;
    default:
      break;
  }
  final key = event.serverKey;
  ref.read(serverProvider(key).notifier).apply(event);
  ref.read(presenceProvider(key).notifier).apply(event);
  ref.read(activityProvider(key).notifier).apply(event);
  ref.read(typingProvider(key).notifier).apply(event);
  ref.read(voiceProvider(key).notifier).apply(event);
  switch (event) {
    case SpeakingChanged(:final speaking):
      ref.read(speakingProvider(key).notifier).set(speaking);
    case MessageCreated(:final message) || MessageUpdated(:final message):
      _toChannel(ref, key, message.channelId, event);
    case MessageDeleted(:final channelId):
      _toChannel(ref, key, channelId, event);
    case OwnVoiceChanged():
      ref.read(voiceSessionProvider.notifier).apply(event);
    case VoiceConnectionChanged():
      ref.read(voiceConnectionProvider.notifier).apply(event);
    default:
      break;
  }
}

void _toChannel(Ref ref, String key, int channelId, RepoEvent event) {
  final channel = (server: key, channel: channelId);
  final messages = channelMessagesProvider(channel);
  if (ref.exists(messages)) ref.read(messages.notifier).apply(event);
  final pins = pinsProvider(channel);
  if (ref.exists(pins)) ref.read(pins.notifier).apply(event);
}

// Servers -------------------------------------------------------------------

const serverOrderKey = 'ui.serverOrder';

/// The saved servers in the user's rail order (§4.1, drag to reorder).
class ServerListNotifier extends Notifier<List<ServerSummary>> {
  @override
  List<ServerSummary> build() =>
      _ordered(ref.watch(repositoryProvider).servers);

  List<String> _savedOrder() {
    final saved = ref.read(keyValueStoreProvider).read(serverOrderKey);
    if (saved == null) return const [];
    try {
      final decoded = jsonDecode(saved);
      return decoded is List ? decoded.whereType<String>().toList() : const [];
    } on FormatException {
      return const [];
    }
  }

  List<ServerSummary> _ordered(List<ServerSummary> servers) {
    final order = _savedOrder();
    final rank = {for (final (index, key) in order.indexed) key: index};
    final indexed = servers.indexed.toList()
      ..sort((a, b) {
        final left = rank[a.$2.key] ?? order.length + a.$1;
        final right = rank[b.$2.key] ?? order.length + b.$1;
        return left.compareTo(right);
      });
    return [for (final (_, server) in indexed) server];
  }

  void refresh() => state = _ordered(ref.read(repositoryProvider).servers);

  void move(String key, int toIndex) {
    final servers = [...state];
    final from = servers.indexWhere((server) => server.key == key);
    if (from == -1) return;
    final moved = servers.removeAt(from);
    servers.insert(toIndex.clamp(0, servers.length), moved);
    state = servers;
    ref
        .read(keyValueStoreProvider)
        .write(serverOrderKey, jsonEncode([for (final s in servers) s.key]));
  }
}

final serverListProvider =
    NotifierProvider<ServerListNotifier, List<ServerSummary>>(
      ServerListNotifier.new,
    );

class ServerNotifier extends Notifier<ServerState> {
  ServerNotifier(this.serverKey);

  final String serverKey;

  @override
  ServerState build() => ServerState(key: serverKey);

  void apply(RepoEvent event) {
    final next = reduceServer(state, event);
    if (!identical(next, state)) state = next;
  }
}

final serverProvider =
    NotifierProvider.family<ServerNotifier, ServerState, String>(
      ServerNotifier.new,
    );

class PresenceNotifier extends Notifier<PresenceState> {
  PresenceNotifier(this.serverKey);

  final String serverKey;

  @override
  PresenceState build() => PresenceState.empty;

  void apply(RepoEvent event) {
    final next = reducePresence(state, event);
    if (!identical(next, state)) state = next;
  }
}

final presenceProvider =
    NotifierProvider.family<PresenceNotifier, PresenceState, String>(
      PresenceNotifier.new,
    );

class ActivityNotifier extends Notifier<ActivityState> {
  ActivityNotifier(this.serverKey);

  final String serverKey;

  @override
  ActivityState build() {
    // Servers without read states of their own remember them through the
    // repository (§6).
    listenSelf((previous, next) {
      if (previous == null || identical(previous, next)) return;
      if (!ref.exists(repositoryProvider)) return;
      final repository = ref.read(repositoryProvider);
      for (final MapEntry(key: id, value: channel) in next.channels.entries) {
        final read = channel.read.lastReadId;
        if (read > (previous.channels[id]?.read.lastReadId ?? 0)) {
          repository.markRead(serverKey, id, read);
        }
      }
    });
    return ActivityState.empty;
  }

  void apply(RepoEvent event) {
    final next = reduceActivity(state, event);
    if (!identical(next, state)) state = next;
  }

  void markRead(int channelId) => state = markChannelRead(state, channelId);

  /// The channel being read right now, or null. Focusing marks it read.
  void focus(int? channelId) {
    state = focusChannel(state, channelId);
    if (channelId != null) markRead(channelId);
  }

  /// Stops reading [channelId], if it is the one being read. Safe to call
  /// while the app shuts down.
  void unfocus(int channelId) {
    if (!ref.mounted || state.focused != channelId) return;
    state = focusChannel(state, null);
  }

  void markAllRead() {
    var next = state;
    for (final id in state.channels.keys) {
      next = markChannelRead(next, id);
    }
    state = next;
  }
}

final activityProvider =
    NotifierProvider.family<ActivityNotifier, ActivityState, String>(
      ActivityNotifier.new,
    );

class TypingNotifier extends Notifier<TypingState> {
  TypingNotifier(this.serverKey);

  final String serverKey;
  Timer? _prune;

  @override
  TypingState build() {
    ref.onDispose(() => _prune?.cancel());
    return TypingState.empty;
  }

  void apply(RepoEvent event) {
    final now = ref.read(clockProvider)();
    final selfId = ref.read(serverProvider(serverKey)).data?.self.id ?? 0;
    final next = reduceTyping(state, event, now: now, selfId: selfId);
    if (identical(next, state)) return;
    state = next;
    _schedulePrune(now);
  }

  void _schedulePrune(DateTime now) {
    _prune?.cancel();
    final expiry = state.nextExpiry;
    if (expiry == null) return;
    _prune = Timer(
      expiry.difference(now) + const Duration(milliseconds: 1),
      () {
        if (!ref.mounted) return;
        final later = ref.read(clockProvider)();
        state = pruneTyping(state, later);
        _schedulePrune(later);
      },
    );
  }
}

final typingProvider =
    NotifierProvider.family<TypingNotifier, TypingState, String>(
      TypingNotifier.new,
    );

// Identity ------------------------------------------------------------------

/// The current user's identity, null until onboarding (Phase 1 §9.1).
class LocalIdentityNotifier extends Notifier<LocalIdentity?> {
  @override
  LocalIdentity? build() {
    final repository = ref.watch(repositoryProvider);
    final changes = repository.events
        .where((event) => event is IdentityChanged)
        .listen((_) => state = repository.identity);
    ref.onDispose(changes.cancel);
    return repository.identity;
  }
}

final localIdentityProvider =
    NotifierProvider<LocalIdentityNotifier, LocalIdentity?>(
      LocalIdentityNotifier.new,
    );

/// Why the saved identity could not be read (the system keyring), or null.
class IdentityUnavailableNotifier extends Notifier<RepoException?> {
  @override
  RepoException? build() {
    final repository = ref.watch(repositoryProvider);
    final changes = repository.events
        .where((event) => event is IdentityChanged)
        .listen((_) => state = repository.identityUnavailable);
    ref.onDispose(changes.cancel);
    return repository.identityUnavailable;
  }
}

final identityUnavailableProvider =
    NotifierProvider<IdentityUnavailableNotifier, RepoException?>(
      IdentityUnavailableNotifier.new,
    );

// Voice (mock only in Phase 1) ---------------------------------------------

class VoiceNotifier extends Notifier<Map<int, List<VoiceParticipant>>> {
  VoiceNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<int, List<VoiceParticipant>> build() => const {};

  void apply(RepoEvent event) {
    switch (event) {
      case Ready(:final snapshot):
        state = snapshot.voice;
      case VoiceChanged(:final channelId, :final participants):
        state = {...state, channelId: participants};
      case ChannelDeleted(:final channelId) when state.containsKey(channelId):
        state = {...state}..remove(channelId);
      default:
        break;
    }
  }
}

final voiceProvider =
    NotifierProvider.family<
      VoiceNotifier,
      Map<int, List<VoiceParticipant>>,
      String
    >(VoiceNotifier.new);

class SpeakingNotifier extends Notifier<Set<int>> {
  SpeakingNotifier(this.serverKey);

  final String serverKey;

  @override
  Set<int> build() => const {};

  void set(Set<int> speaking) {
    if (!setEquals(speaking, state)) state = speaking;
  }
}

/// Who is speaking right now; changes many times a second, so it is kept
/// apart from everything else.
final speakingProvider =
    NotifierProvider.family<SpeakingNotifier, Set<int>, String>(
      SpeakingNotifier.new,
    );

/// The current user's own voice state (§17.1). Mute and deafen hold even
/// outside a voice channel.
@immutable
class VoiceSession {
  const VoiceSession({
    this.serverKey,
    this.channelId,
    this.muted = false,
    this.deafened = false,
    this.camera = false,
    this.screensharing = false,
    this.mutedBeforeDeafen = false,
  });

  final String? serverKey;
  final int? channelId;
  final bool muted;
  final bool deafened;
  final bool camera;
  final bool screensharing;

  /// Restored when undeafening (§17.1).
  final bool mutedBeforeDeafen;

  bool get connected => channelId != null;

  VoiceSession copyWith({
    String? Function()? serverKey,
    int? Function()? channelId,
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
    bool? mutedBeforeDeafen,
  }) => VoiceSession(
    serverKey: serverKey == null ? this.serverKey : serverKey(),
    channelId: channelId == null ? this.channelId : channelId(),
    muted: muted ?? this.muted,
    deafened: deafened ?? this.deafened,
    camera: camera ?? this.camera,
    screensharing: screensharing ?? this.screensharing,
    mutedBeforeDeafen: mutedBeforeDeafen ?? this.mutedBeforeDeafen,
  );
}

class VoiceSessionNotifier extends Notifier<VoiceSession> {
  @override
  VoiceSession build() => const VoiceSession();

  OpencordRepository get _repository => ref.read(repositoryProvider);

  Future<void> join(String serverKey, int channelId) async {
    await _repository.joinVoice(serverKey, channelId);
    state = state.copyWith(
      serverKey: () => serverKey,
      channelId: () => channelId,
      camera: false,
      screensharing: false,
    );
    await _push();
  }

  Future<void> leave() async {
    await _repository.leaveVoice();
    _left();
  }

  void _left() => state = state.copyWith(
    serverKey: () => null,
    channelId: () => null,
    camera: false,
    screensharing: false,
  );

  /// The server moved this device, or it is no longer in voice there.
  void apply(OwnVoiceChanged event) {
    switch (event.channelId) {
      case null when state.serverKey == event.serverKey:
        _left();
      case null:
        break;
      case final channelId:
        state = state.copyWith(
          serverKey: () => event.serverKey,
          channelId: () => channelId,
        );
    }
  }

  Future<void> toggleMute() async {
    if (state.deafened) {
      // Unmuting while deafened also undeafens, like Discord.
      state = state.copyWith(muted: false, deafened: false);
    } else {
      state = state.copyWith(muted: !state.muted);
    }
    await _push();
  }

  Future<void> toggleDeafen() async {
    state = state.deafened
        ? state.copyWith(deafened: false, muted: state.mutedBeforeDeafen)
        : state.copyWith(
            deafened: true,
            mutedBeforeDeafen: state.muted,
            muted: true,
          );
    await _push();
  }

  Future<void> toggleCamera() async {
    state = state.copyWith(camera: !state.camera);
    await _push();
  }

  Future<void> toggleScreenshare() async {
    state = state.copyWith(screensharing: !state.screensharing);
    await _push();
  }

  Future<void> _push() => _repository.setVoiceSelf(
    muted: state.muted,
    deafened: state.deafened,
    camera: state.camera,
    screensharing: state.screensharing,
  );
}

final voiceSessionProvider =
    NotifierProvider<VoiceSessionNotifier, VoiceSession>(
      VoiceSessionNotifier.new,
    );

/// How this device's voice connection is doing (Phase 2 plan §7.14); null
/// outside voice. News can come before the join is done, so the latest per
/// channel is kept.
class VoiceConnectionNotifier extends Notifier<VoiceConnectionStatus?> {
  final _latest = <(String, int), VoiceConnectionStatus>{};

  @override
  VoiceConnectionStatus? build() {
    final (server, channel) = ref.watch(
      voiceSessionProvider.select((s) => (s.serverKey, s.channelId)),
    );
    if (server == null || channel == null) {
      _latest.clear();
      return null;
    }
    return _latest[(server, channel)] ??
        const VoiceConnectionStatus(VoiceConnectionPhase.awaitingEndpoint);
  }

  void apply(VoiceConnectionChanged event) {
    _latest[(event.serverKey, event.channelId)] = event.status;
    final session = ref.read(voiceSessionProvider);
    if (session.serverKey == event.serverKey &&
        session.channelId == event.channelId) {
      state = event.status;
    }
  }
}

final voiceConnectionProvider =
    NotifierProvider<VoiceConnectionNotifier, VoiceConnectionStatus?>(
      VoiceConnectionNotifier.new,
    );

/// The system's microphones and speakers (Phase 2 plan §7.6). Voice media
/// reports changes while in voice; pickers ask again when they open.
class AudioDeviceListNotifier extends Notifier<AudioDeviceList> {
  @override
  AudioDeviceList build() {
    unawaited(refresh());
    return const AudioDeviceList();
  }

  void set(AudioDeviceList devices) => state = devices;

  Future<void> refresh() async {
    try {
      state = await ref.read(repositoryProvider).audioDevices();
    } on RepoException {
      // Listing failed; the list stays as it was.
    }
  }
}

final audioDeviceListProvider =
    NotifierProvider<AudioDeviceListNotifier, AudioDeviceList>(
      AudioDeviceListNotifier.new,
    );

/// Something about voice audio worth a toast, perhaps with an action. A new
/// notice replaces the last, even with the same words.
@immutable
class AudioNotice {
  const AudioNotice(this.message, {this.actionLabel, this.onAction});

  final String message;
  final String? actionLabel;
  final VoidCallback? onAction;
}

class AudioNoticeNotifier extends Notifier<AudioNotice?> {
  @override
  AudioNotice? build() => null;

  void show(String message, {String? actionLabel, VoidCallback? onAction}) =>
      state = AudioNotice(
        message,
        actionLabel: actionLabel,
        onAction: onAction,
      );
}

/// Whether hotkeys work while Opencord is in the background; null until
/// the system answered.
class HotkeySupportNotifier extends Notifier<HotkeySupport?> {
  @override
  HotkeySupport? build() => null;

  void set(HotkeySupport support) => state = support;
}

final hotkeySupportProvider =
    NotifierProvider<HotkeySupportNotifier, HotkeySupport?>(
      HotkeySupportNotifier.new,
    );

/// The microphone's latest level in dBFS, while a meter shows it.
class InputLevelNotifier extends Notifier<double?> {
  @override
  double? build() => null;

  void set(double dbfs) => state = dbfs;
}

final inputLevelProvider = NotifierProvider<InputLevelNotifier, double?>(
  InputLevelNotifier.new,
);

final audioNoticeProvider = NotifierProvider<AudioNoticeNotifier, AudioNotice?>(
  AudioNoticeNotifier.new,
);

// Messages ------------------------------------------------------------------

final _nonces = Random();

/// One channel's history: the newest page loads when first watched, older
/// pages on demand, and sends show at once (§6).
class ChannelMessagesNotifier extends Notifier<ChannelMessages> {
  ChannelMessagesNotifier(this.channel);

  final ChannelRef channel;
  static const pageSize = 50;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// Bumped by each reload: pages asked for before it are dropped.
  var _generation = 0;

  /// The session (server epoch) the newest page was asked for in.
  int? _epoch;
  Timer? _retry;

  /// How many message lists show the channel. One out of sight catches up
  /// when shown again, rather than every channel visited reloading at once
  /// after each new session (the server limits requests).
  var _shown = 0;

  @override
  ChannelMessages build() {
    final server = serverProvider(channel.server);
    _epoch = ref.read(server).epoch;
    ref
      ..listen(server.select((s) => s.epoch), (previous, next) {
        if (previous != null && previous != next && _shown > 0) _reload();
      })
      ..listen(server.select((s) => s.connection.isConnected), (
        previous,
        next,
      ) {
        final failed = state.loadError != null;
        if (next && previous == false && failed && _shown > 0) _reload();
      })
      ..onDispose(() => _retry?.cancel());
    Future.microtask(_loadLatest);
    return ChannelMessages.initial;
  }

  /// A message list shows the channel from now on: catches up on a new
  /// session, or a page that failed, while it was out of sight.
  void show() {
    _shown++;
    final epoch = ref.read(serverProvider(channel.server)).epoch;
    if (state.loadError != null || epoch != _epoch) _reload();
  }

  void hide() {
    if (_shown > 0) _shown--;
  }

  /// The newest page: the first, or again after a new session while the
  /// reader keeps what they have. A failure is kept to show, and tried
  /// again when the server is back or says when to.
  Future<void> _loadLatest() async {
    final generation = _generation;
    _epoch = ref.read(serverProvider(channel.server)).epoch;
    try {
      final page = await _repository.fetchMessages(
        channel.server,
        channel.channel,
        limit: pageSize,
      );
      if (!ref.mounted || generation != _generation) return;
      state = state.loaded
          ? withNewestPage(state, page, limit: pageSize)
          : withPage(state, page, limit: pageSize);
    } on RepoException catch (error) {
      if (!ref.mounted || generation != _generation) return;
      state = state.copyWith(loadError: error);
      if (error.retryAfter case final wait?) {
        _retry?.cancel();
        _retry = Timer(wait, _reload);
      }
    }
  }

  void _reload() {
    _retry?.cancel();
    _generation++;
    _loadLatest();
  }

  /// Tries the newest page again, after it failed.
  void reloadLatest() => _reload();

  Future<void> loadOlder() async {
    if (!state.loaded || !state.hasOlder || state.loadingOlder) return;
    final oldest = state.messages.firstOrNull;
    if (oldest == null) return;
    final generation = _generation;
    state = state.copyWith(loadingOlder: true);
    try {
      final page = await _repository.fetchMessages(
        channel.server,
        channel.channel,
        before: oldest.id,
        limit: pageSize,
      );
      if (!ref.mounted || generation != _generation) return;
      state = withPage(state, page, limit: pageSize);
    } on RepoException {
      if (ref.mounted && generation == _generation) {
        state = state.copyWith(loadingOlder: false);
      }
    }
  }

  void apply(RepoEvent event) {
    final next = reduceMessages(state, event);
    if (!identical(next, state)) state = next;
  }

  Future<void> send(String content, {int? replyToId}) async {
    final text = content.trim();
    if (text.isEmpty) return;
    final now = ref.read(clockProvider)();
    final nonce =
        '${now.microsecondsSinceEpoch.toRadixString(36)}'
        '${_nonces.nextInt(1 << 32).toRadixString(36)}';
    final pending = Message(
      id: -now.microsecondsSinceEpoch,
      channelId: channel.channel,
      authorId: ref.read(serverProvider(channel.server)).data?.self.id ?? 0,
      content: text,
      createdAt: now,
      nonce: nonce,
      replyToId: replyToId,
      sendState: SendState.pending,
    );
    state = withPending(state, pending);
    await _deliver(pending);
  }

  Future<void> _deliver(Message pending) async {
    try {
      final sent = await _repository.sendMessage(
        channel.server,
        channel.channel,
        pending.content,
        nonce: pending.nonce!,
        replyToId: pending.replyToId,
      );
      if (ref.mounted) state = resolvePending(state, sent);
    } on RepoException catch (error) {
      if (!ref.mounted) return;
      state = failPending(state, pending.nonce!);
      if (error.kind == RepoErrorKind.rateLimited) {
        ref
            .read(sendCooldownProvider(channel).notifier)
            .start(error.retryAfter ?? const Duration(seconds: 5));
      }
    }
  }

  /// Sends a failed message again.
  Future<void> retry(String nonce) async {
    final failed = state.pending.where((m) => m.nonce == nonce).firstOrNull;
    if (failed == null) return;
    final again = failed.copyWith(sendState: SendState.pending);
    state = withPending(state, again);
    await _deliver(again);
  }

  void discard(String nonce) => state = removePending(state, nonce);

  Future<void> edit(int messageId, String content) => _repository.editMessage(
    channel.server,
    channel.channel,
    messageId,
    content,
  );

  Future<void> delete(int messageId) =>
      _repository.deleteMessage(channel.server, channel.channel, messageId);

  Future<void> toggleReaction(int messageId, String emoji) => _repository
      .toggleReaction(channel.server, channel.channel, messageId, emoji);

  Future<void> setPinned(int messageId, {required bool pinned}) => _repository
      .setPinned(channel.server, channel.channel, messageId, pinned: pinned);
}

/// When sending works again in a channel after the server said to slow
/// down (§4.6 countdown); null while it works.
class SendCooldownNotifier extends Notifier<DateTime?> {
  SendCooldownNotifier(this.channel);

  final ChannelRef channel;
  Timer? _clear;

  @override
  DateTime? build() {
    ref.onDispose(() => _clear?.cancel());
    return null;
  }

  void start(Duration wait) {
    final safeWait = wait.isNegative ? Duration.zero : wait;
    state = ref.read(clockProvider)().add(safeWait);
    _clear?.cancel();
    _clear = Timer(safeWait, () {
      if (ref.mounted) state = null;
    });
  }
}

final sendCooldownProvider =
    NotifierProvider.family<SendCooldownNotifier, DateTime?, ChannelRef>(
      SendCooldownNotifier.new,
    );

final channelMessagesProvider =
    NotifierProvider.family<
      ChannelMessagesNotifier,
      ChannelMessages,
      ChannelRef
    >(ChannelMessagesNotifier.new);

/// How many members can see a channel and how many of them are online,
/// for the chat header (§4.3).
final channelAudienceProvider =
    Provider.family<({int members, int online}), ChannelRef>((ref, channel) {
      final data = ref.watch(
        serverProvider(channel.server).select((state) => state.data),
      );
      final target = data?.channels[channel.channel];
      if (data == null || target == null) return (members: 0, online: 0);
      final viewers = channelViewers(data, target);
      final presences = ref.watch(
        presenceProvider(channel.server).select((state) => state.presences),
      );
      return (members: viewers.length, online: onlineCount(viewers, presences));
    });

/// A channel's pinned messages, newest first (§4.4), kept current by
/// message events and reloaded after a fresh Ready.
class PinsNotifier extends Notifier<List<Message>> {
  PinsNotifier(this.channel);

  final ChannelRef channel;

  @override
  List<Message> build() {
    ref.listen(serverProvider(channel.server).select((s) => s.epoch), (
      previous,
      next,
    ) {
      if (previous != null && previous != next) _load();
    });
    Future.microtask(_load);
    return const [];
  }

  Future<void> _load() async {
    final repository = ref.read(repositoryProvider);
    if (!repository.capabilities.pins) return;
    try {
      final pins = await repository.fetchPins(channel.server, channel.channel);
      if (ref.mounted) state = sortPins(pins);
    } on RepoException {
      // Pins are extra; the channel works without them.
    }
  }

  void apply(RepoEvent event) {
    final next = reducePins(state, event, channelId: channel.channel);
    if (!identical(next, state)) state = next;
  }
}

final pinsProvider =
    NotifierProvider.family<PinsNotifier, List<Message>, ChannelRef>(
      PinsNotifier.new,
    );

/// Unread messages in channels that are not muted, across all servers: the
/// count in the window title, the tray and the badges (§6, §15).
final unreadTotalProvider = Provider<int>((ref) {
  final prefs = ref.watch(notificationPrefsProvider);
  var total = 0;
  for (final server in ref.watch(serverListProvider)) {
    final activity = ref.watch(activityProvider(server.key));
    for (final MapEntry(key: channel, value: state)
        in activity.channels.entries) {
      if (!prefs.muted(server.key, channel)) total += state.read.unread;
    }
  }
  return total;
});

/// Mentions of the user that are still unread, across all servers: the
/// tray's tooltip and the badges (§15).
final mentionTotalProvider = Provider<int>((ref) {
  var total = 0;
  for (final server in ref.watch(serverListProvider)) {
    total += ref.watch(activityProvider(server.key)).mentions;
  }
  return total;
});

/// Convenience for widgets that only need to know one channel's state.
ChannelActivity channelActivity(WidgetRef ref, ChannelRef channel) => ref.watch(
  activityProvider(channel.server).select((state) => state.of(channel.channel)),
);

/// The read state of a channel without watching it.
ReadState readStateOf(Ref ref, ChannelRef channel) =>
    ref.read(activityProvider(channel.server)).of(channel.channel).read;
