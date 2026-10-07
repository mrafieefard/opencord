import 'dart:convert';

import 'package:opencord/core/settings/key_value_store.dart';

/// How far each channel was read, kept on this device: Phase 1 servers do
/// not track it (§6 unread lines and counts). One entry per server.
class ReadPositions {
  ReadPositions(this._store);

  final KeyValueStore _store;
  final _cache = <String, Map<int, int>>{};

  static String _key(String server) => 'read.$server';

  Map<int, int> _server(String server) => _cache.putIfAbsent(server, () {
    final text = _store.read(_key(server));
    if (text == null) return {};
    try {
      final json = jsonDecode(text);
      if (json is! Map) return {};
      return {
        for (final MapEntry(:key, :value) in json.entries)
          if (int.tryParse('$key') case final channel? when value is int)
            channel: value,
      };
    } on FormatException {
      return {};
    }
  });

  /// The newest message read in [channel], if any is known.
  int? of(String server, int channel) => _server(server)[channel];

  /// Read up to [messageId]; positions only move forward.
  void save(String server, int channel, int messageId) {
    final positions = _server(server);
    if ((positions[channel] ?? 0) >= messageId) return;
    positions[channel] = messageId;
    _store.write(
      _key(server),
      jsonEncode({
        for (final MapEntry(:key, :value) in positions.entries) '$key': value,
      }),
    );
  }

  void forget(String server) {
    _cache.remove(server);
    _store.write(_key(server), null);
  }
}
