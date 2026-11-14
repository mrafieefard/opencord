import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';

/// The activity sessions running in a server's voice channels (plugins M3,
/// design §15.2), by session id. A Ready replaces them all: it holds every
/// session the server runs.
class ActivitySessionsNotifier extends Notifier<Map<int, ActivitySession>> {
  ActivitySessionsNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<int, ActivitySession> build() => const {};

  void apply(RepoEvent event) {
    switch (event) {
      case Ready(:final snapshot):
        state = Map.unmodifiable(snapshot.activitySessions);
      case ActivitySessionStarted(:final session) ||
          ActivitySessionChanged(:final session):
        state = Map.unmodifiable({...state, session.id: session});
      case ActivitySessionEnded(:final sessionId)
          when state.containsKey(sessionId):
        state = Map.unmodifiable({...state}..remove(sessionId));
      case ChannelDeleted(:final channelId)
          when state.values.any((session) => session.channelId == channelId):
        state = Map.unmodifiable({
          for (final MapEntry(:key, :value) in state.entries)
            if (value.channelId != channelId) key: value,
        });
      default:
        break;
    }
  }
}

final activitySessionsProvider =
    NotifierProvider.family<
      ActivitySessionsNotifier,
      Map<int, ActivitySession>,
      String
    >(ActivitySessionsNotifier.new);

/// The ids of the activity sessions this device has opened in its call
/// (plugins M3, §15.3). Emptied when the call's server or channel changes,
/// leaving the call too, and pruned when a session ends.
///
/// While an id is in it, its view (`pluginViewProvider(ActivityPlace)`) is
/// kept, whatever page is shown: paused while no `PluginArea` draws it, and
/// sent the call's members and speakers (the `call` input) as they change.
class OpenActivitiesNotifier extends Notifier<Set<int>> {
  /// At most one `call` input per this, with the newest lists.
  static const callEvery = Duration(milliseconds: 100);

  final _kept = <int, _KeptView>{};
  Timer? _callTimer;

  /// The call the ids are in.
  String? _server;
  int? _channel;

  @override
  Set<int> build() {
    final call = ref.watch(
      voiceSessionProvider.select(
        (voice) => (server: voice.serverKey, channel: voice.channelId),
      ),
    );
    _forgetAll();
    ref.onDispose(_forgetAll);
    _server = call.server;
    _channel = call.channel;
    if (call case (server: final server?, channel: final channel?)) {
      ref.listen(activitySessionsProvider(server), (_, sessions) {
        final ended = {
          for (final id in state)
            if (sessions[id]?.channelId != channel) id,
        };
        if (ended.isEmpty) return;
        ended.forEach(_forget);
        state = state.difference(ended);
      });
      ref.listen(
        voiceProvider(server).select((voice) => voice[channel]),
        (_, _) => _callChanged(),
      );
      ref.listen(speakingProvider(server), (_, _) => _callChanged());
    }
    return const {};
  }

  /// Opens [sessionId]'s view, when it runs in this device's call.
  void open(int sessionId) {
    final server = _server;
    final channel = _channel;
    if (server == null || channel == null || state.contains(sessionId)) {
      return;
    }
    final session = ref.read(activitySessionsProvider(server))[sessionId];
    if (session == null || session.channelId != channel) return;
    final kept = _KeptView();
    _kept[sessionId] = kept;
    kept.subscription = ref.listen(
      pluginViewProvider(ActivityPlace(server, session)),
      (_, controller) => _follow(kept, controller),
      fireImmediately: true,
    );
    state = {...state, sessionId};
  }

  /// Closes this device's view of [sessionId]; the session goes on.
  void close(int sessionId) {
    if (!state.contains(sessionId)) return;
    _forget(sessionId);
    state = {...state}..remove(sessionId);
  }

  /// [controller] runs [kept]'s view: the first, or a new one after Reload.
  void _follow(_KeptView kept, PluginBlocksController controller) {
    kept.controller = controller;
    kept.sent = null;
    controller.keepWithoutArea();
    void opened() {
      if (kept.forgotten || !identical(kept.controller, controller)) return;
      if (controller.viewId.value != null) _sendCall(kept);
    }

    controller.viewId.addListener(opened);
    opened();
  }

  /// The call's members or speakers changed: every opened view hears the
  /// newest lists once [callEvery] has passed.
  void _callChanged() {
    if (_kept.isEmpty || _callTimer != null) return;
    _callTimer = Timer(callEvery, () {
      _callTimer = null;
      if (!ref.mounted) return;
      for (final kept in [..._kept.values]) {
        _sendCall(kept);
      }
    });
  }

  /// Sends [kept]'s view the call's lists, when they differ from what it
  /// was last sent.
  void _sendCall(_KeptView kept) {
    final controller = kept.controller;
    final server = _server;
    final channel = _channel;
    if (controller == null || server == null || channel == null) return;
    if (controller.viewId.value == null) return;
    final members = <int>{
      for (final participant
          in ref.read(voiceProvider(server))[channel] ?? const [])
        participant.userId,
    };
    final call = (
      members: _decimal(members),
      speaking: _decimal(
        ref.read(speakingProvider(server)).where(members.contains),
      ),
    );
    if (kept.sent case final sent?
        when listEquals(sent.members, call.members) &&
            listEquals(sent.speaking, call.speaking)) {
      return;
    }
    kept.sent = call;
    controller.send({
      'kind': 'call',
      'members': call.members,
      'speaking': call.speaking,
    });
  }

  void _forget(int sessionId) {
    final kept = _kept.remove(sessionId);
    if (kept == null) return;
    kept.forgotten = true;
    kept.subscription?.close();
  }

  void _forgetAll() {
    _callTimer?.cancel();
    _callTimer = null;
    for (final sessionId in [..._kept.keys]) {
      _forget(sessionId);
    }
  }
}

/// Ids as the wire format has them: decimal strings, ascending, once each.
List<String> _decimal(Iterable<int> ids) => [
  for (final id in {...ids}.toList()..sort()) '$id',
];

/// One opened session's view, as [OpenActivitiesNotifier] keeps it.
class _KeptView {
  ProviderSubscription<PluginBlocksController>? subscription;

  /// The controller its view runs on now.
  PluginBlocksController? controller;

  /// The `call` lists it was last sent.
  ({List<String> members, List<String> speaking})? sent;

  /// Closed: what still reaches it is ignored.
  bool forgotten = false;
}

final openActivitiesProvider =
    NotifierProvider<OpenActivitiesNotifier, Set<int>>(
      OpenActivitiesNotifier.new,
    );
