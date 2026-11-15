import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `icon` (design §12.2): icons from the host's icon set, by name.
final iconBlock = BlockRenderer(_content);

final _loggedMissingIcons = <String>{};

Widget _content(BlockBuild b) {
  final props = b.props;
  final name = props['name'] as String? ?? '';
  final iconData = b.host.icon(name);
  if (iconData == null) {
    if (name.isNotEmpty && _loggedMissingIcons.add(name)) {
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: Exception('Missing icon: "$name"'),
          library: 'opencord_blocks',
          context: ErrorDescription('while rendering an icon block'),
        ),
      );
    }
    return const SizedBox.shrink();
  }

  final size = numberProp(props['size']) ?? 20.0;
  final color = b.theme.resolveColor(props['color']) ?? b.theme.colors['text']!;

  return Icon(iconData, size: size, color: color);
}
