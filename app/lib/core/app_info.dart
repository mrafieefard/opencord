import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Version reported by the Rust core, like `0.1.0 (protocol v1)`.
final coreVersionProvider = Provider<String>((ref) => 'unknown');

/// The app's own version, as in pubspec.yaml (a test keeps them equal).
const appVersion = '0.1.0';
