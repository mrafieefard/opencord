import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/plugins/plugin_icons.dart';
import 'package:opencord_plugin_host/opencord_plugin_host.dart';

void main() {
  // Fails if an icon is added to, renamed in or changed in one list and
  // not the other: the phone would draw another glyph, or none.
  test("the phone's icon names are the desktop's", () {
    expect(pluginIconGlyphs.keys.toSet(), pluginIcons.keys.toSet());
    for (final MapEntry(key: name, value: icon) in pluginIcons.entries) {
      expect(pluginIconGlyphs[name]?.codePoint, icon.codePoint, reason: name);
      expect(pluginIconGlyphs[name]?.fontFamily, icon.fontFamily, reason: name);
    }
  });
}
