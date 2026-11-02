import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

/// The call this device is in, kept while in it, so that a start after the
/// app closed in a call can offer it again (D88).
const lastCallKey = 'voice.lastCall';

/// Forgets the call before quitting on purpose, so the next start offers
/// nothing.
void forgetLastCall(KeyValueStore store) => store.write(lastCallKey, null);

/// Why this device is out of a call it didn't leave.
enum VoiceDropReason {
  /// The app closed while in it.
  closed,

  /// Another of the user's devices took it over.
  elsewhere,

  /// Getting back in after a lost connection was refused.
  refused,
}

/// A call to offer reconnecting to (D88).
@immutable
class VoiceDrop {
  const VoiceDrop({
    required this.serverKey,
    required this.channelId,
    required this.reason,
    this.message,
  });

  final String serverKey;
  final int channelId;
  final VoiceDropReason reason;

  /// The server's reason, for [VoiceDropReason.refused].
  final String? message;

  @override
  bool operator ==(Object other) =>
      other is VoiceDrop &&
      other.serverKey == serverKey &&
      other.channelId == channelId &&
      other.reason == reason &&
      other.message == message;

  @override
  int get hashCode => Object.hash(serverKey, channelId, reason, message);

  @override
  String toString() => 'VoiceDrop($serverKey, $channelId, $reason, $message)';
}

/// The reconnect bar's offer: set when this device is out of a call it
/// didn't leave, gone once it is in any call again or the offer is
/// dismissed. Leaving, and a moderator's disconnect, offer nothing.
class VoiceReconnectNotifier extends Notifier<VoiceDrop?> {
  KeyValueStore get _store => ref.read(keyValueStoreProvider);

  @override
  VoiceDrop? build() {
    ref.listen(
      voiceSessionProvider.select((voice) => (voice.serverKey, voice.channelId)),
      (_, call) {
        if (call case (final String server, final int channel)) {
          _store.write(
            lastCallKey,
            jsonEncode({'server': server, 'channel': channel}),
          );
          state = null;
        } else {
          forgetLastCall(_store);
        }
      },
    );
    return _closedIn();
  }

  /// The call the app closed in, if it did; offered once.
  VoiceDrop? _closedIn() {
    final saved = readJson(_store, lastCallKey);
    forgetLastCall(_store);
    if (saved case {
      'server': final String server,
      'channel': final int channel,
    }) {
      return VoiceDrop(
        serverKey: server,
        channelId: channel,
        reason: VoiceDropReason.closed,
      );
    }
    return null;
  }

  /// Before the session hears [event]: another device taking the call
  /// leaves an offer to take it back.
  void ownVoiceChanged(OwnVoiceChanged event) {
    final voice = ref.read(voiceSessionProvider);
    final channel = voice.channelId;
    if (!event.elsewhere || voice.serverKey != event.serverKey) return;
    if (channel == null) return;
    state = VoiceDrop(
      serverKey: event.serverKey,
      channelId: channel,
      reason: VoiceDropReason.elsewhere,
    );
  }

  void rejoinFailed(VoiceRejoinFailed event) => state = VoiceDrop(
    serverKey: event.serverKey,
    channelId: event.channelId,
    reason: VoiceDropReason.refused,
    message: event.message,
  );

  /// Joins the call again, taking it back from another device if need be.
  /// A refusal throws [RepoException] and the offer stays.
  Future<void> reconnect() async {
    final drop = state;
    if (drop == null) return;
    await ref
        .read(voiceSessionProvider.notifier)
        .join(drop.serverKey, drop.channelId);
    if (state == drop) state = null;
  }

  void dismiss() => state = null;
}

final voiceReconnectProvider =
    NotifierProvider<VoiceReconnectNotifier, VoiceDrop?>(
      VoiceReconnectNotifier.new,
    );
