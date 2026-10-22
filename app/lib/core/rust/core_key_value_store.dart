import 'package:flutter/foundation.dart';

import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/src/rust/api/types.dart';

/// Settings in the Rust core's local data file. The core must be
/// initialized first.
final class CoreKeyValueStore implements KeyValueStore {
  const CoreKeyValueStore();

  @override
  String? read(String key) {
    try {
      return core.settingsGet(key: key);
    } on CoreError catch (error) {
      debugPrint('could not read setting $key: $error');
      return null;
    }
  }

  @override
  void write(String key, String? value) {
    try {
      core.settingsSet(key: key, value: value);
    } on CoreError catch (error) {
      debugPrint('could not save setting $key: $error');
    }
  }
}
