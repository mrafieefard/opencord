import 'dart:math' as math;
import 'dart:ui';

import 'package:opencord/core/model/misc.dart';

/// A tile in the voice view: someone, or the screen they share.
typedef VoiceTileId = ({int userId, bool screen});

/// One tile per participant, and each screenshare right after the person
/// sharing it (§4.10).
List<VoiceTileId> voiceTiles(List<VoiceParticipant> participants) => [
  for (final participant in participants) ...[
    (userId: participant.userId, screen: false),
    if (participant.screensharing) (userId: participant.userId, screen: true),
  ],
];

/// Grid columns for [count] tiles (§4.10): 1 → 1, up to 4 → 2, up to 9 → 3,
/// more → 4.
int gridColumns(int count) => switch (count) {
  <= 1 => 1,
  <= 4 => 2,
  <= 9 => 3,
  _ => 4,
};

/// The largest 16:9 tile that lets [count] tiles fit in [area], [gap]
/// apart, without scrolling.
Size fitTiles(Size area, int count, {double gap = 8}) {
  if (count <= 0) return Size.zero;
  final columns = gridColumns(count);
  final rows = (count / columns).ceil();
  final byWidth = (area.width - gap * (columns - 1)) / columns;
  final byHeight = (area.height - gap * (rows - 1)) / rows * 16 / 9;
  final width = math.min(byWidth, byHeight);
  if (width <= 0) return Size.zero;
  return Size(width, width * 9 / 16);
}
