import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/servers/server_rail.dart';

/// The rail's item for a server, or `Private` for the private messages, by
/// the name it stands for rather than any text it draws.
Finder railItem(String name) =>
    find.byWidgetPredicate((w) => w is RailItemView && w.name == name);
