import 'dart:typed_data';

import 'package:flutter/widgets.dart';

/// What the renderer asks of the app around it. The package knows nothing of
/// the app's state, its repository or the core: the desktop app (and, from
/// milestone 4, the phone's Flutter module) implements this.
abstract class BlocksHost {
  /// An icon by its plugin name, or null for a name the app doesn't have.
  IconData? icon(String name);

  /// An image from one of an `image` block's sources (design §13.1).
  Future<ImageProvider?> image(Map<String, Object?> source);

  /// A member's avatar, and their name beside it when [showName].
  Widget user(String id, double size, bool showName);

  /// Message markdown, drawn as messages are.
  Widget richText(String markdown, TextStyle style);

  /// Lets the viewer choose a file; null when they cancel.
  Future<String?> pickFile({required bool image, List<String> accept});

  /// Asks the viewer whether to open [url], and opens it when they agree.
  Future<bool> confirmLink(String url);

  void copy(String text);

  void openChannel(String id);

  /// Plays a sound from the plugin's assets; [onEnd] runs when it ends.
  Future<void> playSound(String asset, double volume, VoidCallback onEnd);

  /// A font from the plugin's `assets/fonts/`.
  Future<ByteData?> font(String file);

  /// Uploads a picker's file; the core answers with the block's `upload` or
  /// `upload_error` event.
  Future<void> upload(int nodeId, String path);
}
