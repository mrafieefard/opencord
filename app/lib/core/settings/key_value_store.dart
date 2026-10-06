import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Small string settings that survive restarts. The app uses the Rust
/// core's settings file; tests use [MemoryKeyValueStore].
abstract interface class KeyValueStore {
  String? read(String key);

  /// `null` removes the key.
  void write(String key, String? value);
}

final class MemoryKeyValueStore implements KeyValueStore {
  final Map<String, String> _values = {};

  @override
  String? read(String key) => _values[key];

  @override
  void write(String key, String? value) {
    if (value == null) {
      _values.remove(key);
    } else {
      _values[key] = value;
    }
  }
}

/// Overridden at startup with the store the app should use.
final keyValueStoreProvider = Provider<KeyValueStore>(
  (ref) => throw StateError('keyValueStoreProvider must be overridden'),
);

/// The platform the app runs on, overridable in tests.
final platformProvider = Provider<TargetPlatform>(
  (ref) => defaultTargetPlatform,
);
