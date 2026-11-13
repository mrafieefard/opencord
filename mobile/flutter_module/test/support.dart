import 'dart:convert';

/// Copied from `app/packages/opencord_blocks/test/support.dart`: batches as
/// the core sends them.
String batch(int seq, List<Map<String, Object?>> ops) =>
    jsonEncode({'view': '1', 'seq': seq, 'ops': ops});

Map<String, Object?> create(
  int id,
  String block, {
  String? key,
  Map<String, Object?> props = const {},
  List<String> events = const [],
  List<int> children = const [],
}) => {
  'op': 'create',
  'id': id,
  'block': block,
  'key': ?key,
  'props': props,
  'events': events,
  'children': children,
};

Map<String, Object?> roots(int main) => {
  'op': 'roots',
  'main': main,
  'modals': const <Object?>[],
};

/// A setup as the native side sends it: the desktop's dark colours.
String setupJson({String theme = 'dark'}) => jsonEncode({
  'theme': theme,
  'colors': {
    'text': '#EDEDEF',
    'text_secondary': '#A1A1A8',
    'text_muted': '#6E6E76',
    'background': '#0E0E10',
    'surface': '#18181B',
    'elevated': '#1F1F23',
    'border': '#222226',
    'accent': '#EDEDEF',
    'on_accent': '#0E0E10',
    'selected': '#27272C',
    'hover': '#1C1C20',
    'danger': '#F2555A',
  },
  'text_scale': 1.0,
  'safe_area': {'top': 0, 'right': 0, 'bottom': 0, 'left': 0},
  'reduce_motion': false,
  'server_key': 'server-a',
  'plugin_id': 'tickets',
});
