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

const _dark = {
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
};

const _light = {
  'text': '#111114',
  'text_secondary': '#55555C',
  'text_muted': '#8A8A92',
  'background': '#F3F3F5',
  'surface': '#FFFFFF',
  'elevated': '#FFFFFF',
  'border': '#E3E3E7',
  'accent': '#111114',
  'on_accent': '#FFFFFF',
  'selected': '#E7E7EA',
  'hover': '#F2F2F4',
  'danger': '#DC3E42',
};

/// A setup as the native side sends it: the phone's colours, which are the
/// desktop's (mobile/src/ui/theme/tokens.ts).
String setupJson({String theme = 'dark'}) => jsonEncode({
  'theme': theme,
  'colors': theme == 'light' ? _light : _dark,
  'text_scale': 1.0,
  'safe_area': {'top': 0, 'right': 0, 'bottom': 0, 'left': 0},
  'reduce_motion': false,
  'server_key': 'server-a',
  'plugin_id': 'tickets',
});
