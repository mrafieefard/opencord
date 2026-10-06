import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Version reported by the Rust core, like `0.1.0 (protocol v1)`.
final coreVersionProvider = Provider<String>((ref) => 'unknown');
