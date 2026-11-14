import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `user` (design §12.2): avatar and name from the app's cache through
/// [BlocksHost.user].
final userBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final props = b.props;
  final id = props['id'] as String? ?? '';
  final size = numberProp(props['size']) ?? 32.0;
  final showName = props['show_name'] == true;

  return b.host.user(id, size, showName);
}
