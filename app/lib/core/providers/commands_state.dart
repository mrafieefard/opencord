import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';

/// How long a deferred command may take; the server decides, this is only
/// when a private "thinking" line stops waiting for it.
const thinkingLimit = Duration(minutes: 15);

/// A little longer than the server, whose answer should arrive first.
const _thinkingGrace = Duration(seconds: 30);

/// A channel on a server, as the command providers key it.
typedef CommandChannel = ({String server, int channel});

/// Replies only this device sees, per channel, until dismissed or the app
/// closes (Phase 3 plan §17.3); never stored.
class EphemeralNotesNotifier extends Notifier<List<EphemeralNote>> {
  EphemeralNotesNotifier(this.channel);

  final CommandChannel channel;

  /// Kept per channel.
  static const kept = 20;

  final _expiries = <EphemeralNote, Timer>{};

  @override
  List<EphemeralNote> build() {
    ref.onDispose(_cancelAll);
    // A reply sent while the connection was down is lost, so what was
    // thinking then will never finish.
    ref.listen(
      serverProvider(channel.server).select((state) => state.connection.phase),
      (previous, phase) {
        if (phase == ConnectionPhase.connected &&
            previous != ConnectionPhase.connected) {
          _dropThinking();
        }
      },
    );
    return const [];
  }

  void _cancelAll() {
    for (final timer in _expiries.values) {
      timer.cancel();
    }
    _expiries.clear();
  }

  void _dropThinking() {
    for (final note in state) {
      if (note.state == CommandRunState.thinking) {
        _expiries.remove(note)?.cancel();
      }
    }
    state = [
      for (final note in state)
        if (note.state != CommandRunState.thinking) note,
    ];
  }

  void add(EphemeralNote note) {
    final replaced = [
      for (final each in state)
        if (each.nonce == note.nonce && note.nonce.isNotEmpty) each,
    ];
    for (final each in replaced) {
      _expiries.remove(each)?.cancel();
    }
    final rest = [
      for (final each in state)
        if (!replaced.contains(each)) each,
    ];
    final next = [...rest, note];
    state = next.length > kept ? next.sublist(next.length - kept) : next;
    if (note.state == CommandRunState.thinking) {
      _expiries[note] = Timer(
        thinkingLimit + _thinkingGrace,
        () => dismiss(note),
      );
    }
  }

  void dismiss(EphemeralNote note) {
    _expiries.remove(note)?.cancel();
    state = [
      for (final each in state)
        if (!identical(each, note)) each,
    ];
  }
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
