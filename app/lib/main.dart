import 'dart:io';

import 'package:flutter/widgets.dart';

import 'package:opencord/app_start.dart';
import 'package:opencord/core/profile.dart';
import 'package:opencord/features/window/native_window.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final environment = Platform.environment;
  runApp(
    await startupWidget(
      () => startApp(
        profile: appProfile(environment),
        mock: usesMock(args, environment),
        links: [
          for (final arg in args)
            if (arg.startsWith('opencord:')) arg,
        ],
      ),
      window: ChannelNativeWindow.new,
    ),
  );
}
