import 'dart:math' as math;
import 'dart:ui';

import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/plugins.dart';

/// A tile in the voice view: someone, the screen they share, or an activity
/// session running in the call (plugins M3, §15.2). A person's and a
/// screen's have `session: null`; an activity's has `userId: 0`,
/// `screen: false` and its session's id.
typedef VoiceTileId = ({int userId, bool screen, int? session});

/// The tile of someone ([screen] false) or of the screen they share.
VoiceTileId personTile(int userId, {bool screen = false}) =>
    (userId: userId, screen: screen, session: null);

/// The tile of the activity session [sessionId].
VoiceTileId sessionTile(int sessionId) =>
    (userId: 0, screen: false, session: sessionId);

/// One tile per participant, and each screenshare right after the person
/// sharing it (§4.10). With [videoOnly] (Phase 2 §12, "Hide non-video
/// participants"), only cameras and screenshares. The call's activity
/// [sessions] come after all of them, oldest first, and stay with
/// [videoOnly].
List<VoiceTileId> voiceTiles(
  List<VoiceParticipant> participants, {
  bool videoOnly = false,
  Iterable<ActivitySession> sessions = const [],
}) => [
  for (final participant in participants) ...[
    if (!videoOnly || participant.camera) personTile(participant.userId),
    if (participant.screensharing) personTile(participant.userId, screen: true),
  ],
  for (final session in [...sessions]..sort(_oldestFirst))
    sessionTile(session.id),
];

int _oldestFirst(ActivitySession a, ActivitySession b) {
  final byStart = a.startedAt.compareTo(b.startedAt);
  return byStart != 0 ? byStart : a.id.compareTo(b.id);
}

/// Grid columns for [count] tiles (§4.10): 1 → 1, up to 4 → 2, up to 9 → 3,
/// more → 4.
int gridColumns(int count) => switch (count) {
  <= 1 => 1,
  <= 4 => 2,
  <= 9 => 3,
  _ => 4,
};

/// Between tiles, in the grid and the strip.
const double tileGap = 8;

/// The stage's padding around the grid, and around focus mode (§5.14).
const double gridPadding = 12, focusPadding = 8;

const double _tileAspect = 16 / 10;
const double _groupWidth = 1040, _fewGroupWidth = 520;
const int _fewTiles = 3;

/// The prototype's grid gives no tile less than this, and takes a column away
/// first.
const double _tileMinWidth = 190;

const double _stripHeight = 76, _stripTileMin = 96, _stripTileMax = 200;

double _groupLimit(double width, int count) =>
    math.min(width, count < _fewTiles ? _fewGroupWidth : _groupWidth);

/// [gridColumns], as many as fit in [width] at the narrowest a tile may be.
int tileColumns(int count, double width, {double gap = tileGap}) {
  final fit = ((width + gap) / (_tileMinWidth + gap)).floor();
  return math.max(1, math.min(gridColumns(count), fit));
}

/// The largest 16:10 tile that lets [count] tiles fit in [area], [gap]
/// apart, without scrolling, in a group at most 1040 wide, or 520 for a few.
Size fitTiles(Size area, int count, {double gap = tileGap}) {
  if (count <= 0) return Size.zero;
  final limit = _groupLimit(area.width, count);
  final columns = tileColumns(count, limit, gap: gap);
  final rows = (count / columns).ceil();
  final byWidth = (limit - gap * (columns - 1)) / columns;
  final byHeight = (area.height - gap * (rows - 1)) / rows * _tileAspect;
  final width = math.min(byWidth, byHeight);
  if (width <= 0) return Size.zero;
  return Size(width, width / _tileAspect);
}

/// Where a grid of [count] tiles goes in [area]: the group, centred, and each
/// tile in it. A short last row starts at the left, as the prototype's does.
({Rect group, List<Rect> tiles}) gridLayout(Size area, int count) {
  final tile = fitTiles(area, count);
  final columns = tileColumns(count, _groupLimit(area.width, count));
  final rows = count == 0 ? 0 : (count / columns).ceil();
  final group = Size(
    columns * tile.width + (columns - 1) * tileGap,
    math.max(0, rows * tile.height + (rows - 1) * tileGap),
  );
  return (
    group: Rect.fromCenter(
      center: area.center(Offset.zero),
      width: group.width,
      height: group.height,
    ),
    tiles: [
      for (var index = 0; index < count; index++)
        Offset(
              (index % columns) * (tile.width + tileGap),
              (index ~/ columns) * (tile.height + tileGap),
            ) &
            tile,
    ],
  );
}

/// Focus mode in [area]: one thing above a strip of [others] tiles, which
/// share the width and scroll past the narrowest they may be. Without others
/// there's no strip, and the large one has it all.
({Rect large, Rect strip, Size stripTile}) focusLayout(Size area, int others) {
  if (others == 0) {
    return (large: Offset.zero & area, strip: Rect.zero, stripTile: Size.zero);
  }
  final shared = (area.width - tileGap * (others - 1)) / others;
  final top = math.max(0.0, area.height - _stripHeight);
  return (
    large: Rect.fromLTWH(0, 0, area.width, math.max(0, top - tileGap)),
    strip: Rect.fromLTWH(
      0,
      top,
      area.width,
      math.min(_stripHeight, area.height),
    ),
    stripTile: Size(
      shared.clamp(_stripTileMin, _stripTileMax),
      math.min(_stripHeight, area.height),
    ),
  );
}
