import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/app.dart';
import 'package:opencord/app_start.dart';
import 'package:opencord/core/profile.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final environment = Platform.environment;
  final container = await startApp(
    profile: appProfile(environment),
    mock: usesMock(args, environment),
    links: [
      for (final arg in args)
        if (arg.startsWith('opencord:')) arg,
    ],
  );
  runApp(
    UncontrolledProviderScope(container: container, child: const OpencordApp()),
  );
}
