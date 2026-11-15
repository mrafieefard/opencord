import 'package:flutter/widgets.dart';

import '../view.dart';

/// A block this renderer doesn't draw. The core already refused names that
/// aren't blocks, so this is a block of a newer app, drawn as text in the
/// error style rather than nothing.
final unknownBlock = BlockRenderer(
  (b) => Text('Unknown block: ${b.node.block}', style: b.theme.errorText),
);
