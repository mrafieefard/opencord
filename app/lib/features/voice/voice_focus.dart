import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/voice/voice_layout.dart';

/// The tile shown large in a voice channel's focus mode (§4.10), or null
/// for the grid. Forgotten once the voice view closes.
class VoiceFocusNotifier extends Notifier<VoiceTileId?> {
  VoiceFocusNotifier(this.channel);

  final ChannelRef channel;

  @override
  VoiceTileId? build() => null;

  /// Focuses [tile], or returns to the grid when it is already focused.
  void toggle(VoiceTileId tile) => state = state == tile ? null : tile;

  void clear() => state = null;
}

final voiceFocusProvider = NotifierProvider.autoDispose
    .family<VoiceFocusNotifier, VoiceTileId?, ChannelRef>(
      VoiceFocusNotifier.new,
    );
