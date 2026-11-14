import 'package:flutter/widgets.dart';

import 'src/area.dart';
import 'src/channel.dart';

/// One engine per plugin area; React Native's native module starts it and
/// talks to it over [AreaChannel].
void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(PluginAreaApp(channel: AreaChannel()));
}
