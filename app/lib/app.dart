import 'package:flutter/material.dart';

/// Root widget of the Opencord client.
///
/// Until the real UI lands (M5) it shows a placeholder proving the Rust core
/// is wired in.
class OpencordApp extends StatelessWidget {
  const OpencordApp({super.key, required this.coreVersion});

  /// Version string reported by the Rust core.
  final String coreVersion;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Opencord',
      debugShowCheckedModeBanner: false,
      themeMode: ThemeMode.dark,
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF14B8A6),
          brightness: Brightness.dark,
        ),
      ),
      home: _PlaceholderScreen(coreVersion: coreVersion),
    );
  }
}

class _PlaceholderScreen extends StatelessWidget {
  const _PlaceholderScreen({required this.coreVersion});

  final String coreVersion;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Opencord', style: theme.textTheme.displaySmall),
            const SizedBox(height: 8),
            Text(
              'Core $coreVersion',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
