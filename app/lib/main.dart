import 'dart:io';

import 'package:flutter/widgets.dart';

import 'package:opencord/app_start.dart';
import 'package:opencord/core/profile.dart';
import 'package:opencord/features/desktop/login_item.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/ui/widgets/error_box.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  // Here rather than in startApp, which the end-to-end tests share: their
  // binding wants Flutter's own error box back when they end.
  limitErrorBoxes();
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
        atLogin: args.contains(autostartArg),
      ),
      window: ChannelNativeWindow.new,
    ),
  );
}
