import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:opencord_blocks/opencord_blocks.dart';
import 'package:opencord_plugin_host/opencord_plugin_host.dart';

import 'channel.dart';

/// What the renderer asks of the phone app. Icons and rich text are drawn
/// here; the rest goes to React Native (Task 9). Until then those members
/// answer what the renderer treats as nothing, and never throw.
class PhoneBlocksHost implements BlocksHost {
  PhoneBlocksHost({required this.channel, required this.theme});

  final AreaChannel channel;
  final BlocksTheme theme;

  @override
  IconData? icon(String name) => pluginIconGlyphs[name];

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async => null;

  @override
  Widget user(String id, double size, bool showName) =>
      Text(id, style: theme.text);

  @override
  Widget richText(String markdown, TextStyle style) => MarkdownText(
    markdown: markdown,
    style: style,
    linkColor: theme.colors['accent']!,
    codeBackground: theme.colors['elevated']!,
  );

  @override
  Future<String?> pickFile({
    required bool image,
    List<String> accept = const [],
  }) async => null;

  @override
  Future<bool> confirmLink(String url) async => false;

  @override
  void copy(String text) => _later('copy');

  @override
  void openChannel(String id) => _later('openChannel');

  @override
  Future<void> playSound(
    String asset,
    double volume,
    VoidCallback onEnd,
  ) async => _later('playSound');

  @override
  Future<ByteData?> font(String file) async => null;

  @override
  Future<void> upload(int nodeId, String path) async => _later('upload');

  void _later(String method) =>
      debugPrint('plugin area: $method is not answered yet');
}
