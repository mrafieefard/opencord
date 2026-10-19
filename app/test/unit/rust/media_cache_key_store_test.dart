import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/rust/media_cache_key_store.dart';

const _plugin = MethodChannel('plugins.it_nomads.com/flutter_secure_storage');

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final keychain = <String, String>{};

  setUp(() {
    keychain.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_plugin, (call) async {
          final arguments = call.arguments as Map<Object?, Object?>;
          final key = arguments['key']! as String;
          return switch (call.method) {
            'read' => keychain[key],
            'write' => keychain[key] = arguments['value']! as String,
            _ => null,
          };
        });
  });

  tearDown(
    () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_plugin, null),
  );

  test('the key is made once, then reused', () async {
    final first = await MediaCacheKeyStore().load();
    final second = await MediaCacheKeyStore().load();

    expect(first, hasLength(32));
    expect(second, first);
    expect(keychain.keys, ['opencord.media-cache.key']);
  });

  test('a damaged key is replaced', () async {
    keychain['opencord.media-cache.key'] = base64Encode([1, 2, 3]);

    final key = await MediaCacheKeyStore().load();

    expect(key, hasLength(32));
    expect(base64Decode(keychain['opencord.media-cache.key']!), key);
  });

  test('a profile keeps its own key', () async {
    final main = await MediaCacheKeyStore().load();
    final other = await MediaCacheKeyStore(profile: 'b').load();

    expect(other, isNot(main));
    expect(keychain.keys, contains('opencord.b.media-cache.key'));
  });
}
