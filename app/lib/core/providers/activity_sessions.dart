import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/plugins.dart';
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
