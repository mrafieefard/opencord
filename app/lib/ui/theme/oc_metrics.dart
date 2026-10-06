import 'package:flutter/widgets.dart';

/// Spacing scale from §2.3.
abstract final class OcSpace {
  static const double s2 = 2;
  static const double s4 = 4;
  static const double s6 = 6;
  static const double s8 = 8;
  static const double s10 = 10;
  static const double s12 = 12;
  static const double s16 = 16;
  static const double s20 = 20;
  static const double s24 = 24;
  static const double s32 = 32;
}

/// Corner radii from §2.3 and the component sections of §4.
abstract final class OcRadius {
  static const double row = 10;
  static const double input = 10;
  static const double searchPill = 18;
  static const double composer = 20;
  static const double bubble = 16;
  static const double bubbleGrouped = 5;
  static const double dialog = 16;
  static const double menu = 10;
  static const double serverIcon = 23;
  static const double serverIconActive = 14;
  static const double channelGlyph = 12;
  static const double reaction = 12;
  static const double quote = 6;
  static const double section = 12;
  static const double tile = 14;
}

/// Fixed sizes from §2.3, §3 and §4.
abstract final class OcSize {
  static const double iconInline = 16;
  static const double iconRow = 18;
  static const double iconButton = 20;

  static const double hitDefault = 36;
  static const double hitCompact = 28;
  static const double hitVoice = 48;

  static const double railWidth = 76;
  static const double sidebarWidth = 304;
  static const double sidebarNarrowWidth = 264;
  static const double memberPanelWidth = 264;
  static const double header = 60;
  static const double userPanel = 58;
  static const double channelRow = 56;
  static const double pinnedBar = 46;
  static const double messageColumn = 880;
  static const double bubbleMaxWidth = 560;

  static const double serverIcon = 46;
  static const double channelGlyph = 40;
  static const double messageAvatar = 36;
  static const double memberAvatar = 34;
  static const double voiceAvatar = 22;
  static const double jumpButton = 44;

  static const Size minWindow = Size(940, 560);
  static const Size defaultWindow = Size(1280, 800);
}

/// Window widths where the layout changes (§3).
abstract final class OcBreakpoint {
  /// All four columns from here up.
  static const double fourColumns = 1180;

  /// Below this the sidebar narrows to 264 px.
  static const double narrowSidebar = 980;

  /// Below this the sidebar becomes an overlay.
  static const double collapsedSidebar = 760;
}
