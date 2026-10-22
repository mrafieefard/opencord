import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/repository/events.dart';

/// Presence and activity of a server's members. Kept apart from the rest of
/// the server state so a presence change only rebuilds what shows it.
@immutable
class PresenceState {
  const PresenceState({this.presences = const {}, this.activities = const {}});

  static const empty = PresenceState();

  final Map<int, Presence> presences;

  /// Free text like "Editing main.rs".
  final Map<int, String> activities;

  Presence of(int userId) => presences[userId] ?? Presence.offline;
}

PresenceState reducePresence(PresenceState state, RepoEvent event) {
  switch (event) {
    case Ready(:final snapshot):
      return PresenceState(
        presences: snapshot.presences,
        activities: snapshot.activities,
      );
    case PresenceChanged(:final userId, :final presence, :final activity):
      final activities = {...state.activities};
      if (activity == null) {
        activities.remove(userId);
      } else {
        activities[userId] = activity;
      }
      return PresenceState(
        presences: {...state.presences, userId: presence},
        activities: activities,
      );
    case MemberLeft(:final userId):
      return PresenceState(
        presences: {...state.presences}..remove(userId),
        activities: {...state.activities}..remove(userId),
      );
    default:
      return state;
  }
}
