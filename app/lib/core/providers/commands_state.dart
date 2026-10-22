import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/repository/repository.dart';

/// A channel on a server, as the command providers key it.
typedef CommandChannel = ({String server, int channel});

/// Replies only this device sees, per channel, until dismissed or the app
/// closes (Phase 3 plan §17.3); never stored.
class EphemeralNotesNotifier extends Notifier<List<EphemeralNote>> {
  EphemeralNotesNotifier(this.channel);

  final CommandChannel channel;

  /// Kept per channel.
  static const kept = 20;

  @override
  List<EphemeralNote> build() => const [];

  void add(EphemeralNote note) {
    final rest = [
      for (final each in state)
        if (each.nonce != note.nonce || note.nonce.isEmpty) each,
    ];
    final next = [...rest, note];
    state = next.length > kept ? next.sublist(next.length - kept) : next;
  }

  void dismiss(EphemeralNote note) => state = [
    for (final each in state)
      if (!identical(each, note)) each,
  ];
}

final ephemeralNotesProvider =
    NotifierProvider.family<
      EphemeralNotesNotifier,
      List<EphemeralNote>,
      CommandChannel
    >(EphemeralNotesNotifier.new);

/// Routes a command's "only you" reply to its channel.
void routeCommandEvent(Ref ref, RepoEvent event) {
  if (event case EphemeralMessageReceived(:final serverKey, :final note)) {
    ref
        .read(
          ephemeralNotesProvider((
            server: serverKey,
            channel: note.channelId,
          )).notifier,
        )
        .add(note);
  }
}
