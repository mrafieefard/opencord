import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// What shows when Opencord cannot start at all (its data folder cannot
/// be opened, say): why, and Quit, instead of a window that never appears.
class StartupFailedApp extends StatelessWidget {
  const StartupFailedApp({
    super.key,
    required this.reason,
    required this.onQuit,
  });

  final String reason;
  final VoidCallback onQuit;

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Opencord',
    debugShowCheckedModeBanner: false,
    theme: buildTheme(OcColors.light),
    darkTheme: buildTheme(OcColors.dark),
    themeAnimationDuration: Duration.zero,
    home: Builder(
      builder: (context) {
        final colors = context.oc;
        return Material(
          color: colors.chat,
          child: Center(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(OcSpace.s24),
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 440),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Icon(OcIcons.error, size: 48, color: colors.text),
                    const SizedBox(height: OcSpace.s16),
                    Text(
                      'Opencord could not start',
                      textAlign: TextAlign.center,
                      style: OcText.title.copyWith(color: colors.text),
                    ),
                    const SizedBox(height: OcSpace.s8),
                    SelectableText(
                      reason,
                      textAlign: TextAlign.center,
                      style: OcText.body.copyWith(color: colors.textSecondary),
                    ),
                    const SizedBox(height: OcSpace.s24),
                    OcButton.primary(
                      label: 'Quit',
                      expand: true,
                      autofocus: true,
                      onPressed: onQuit,
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    ),
  );
}
