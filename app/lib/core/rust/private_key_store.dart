import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

/// The 32-byte master key the private messages SQLite database and Olm session
/// storage are sealed with (private messages plan, Task 16). It stays in the
/// system keychain, beside the identity, so private conversations, messages,
/// sessions, and contacts cannot be read without this user's login. [profile] keeps
/// a second instance's key apart, as [SecureIdentityStore] does.
class PrivateKeyStore {
  PrivateKeyStore({String profile = ''})
    : _name = profile.isEmpty
          ? 'opencord.private.key'
          : 'opencord.$profile.private.key';

  static const keyLength = 32;

  final String _name;

  // The same options as SecureIdentityStore: on macOS the login keychain.
  final _storage = const FlutterSecureStorage(
    mOptions: MacOsOptions(usesDataProtectionKeychain: false),
  );

  /// The saved key, or a new random one, saved first. Throws the plugin's
  /// PlatformException when the keychain can't be used.
  Future<Uint8List> load() async {
    final saved = await _storage.read(key: _name);
    if (saved != null) {
      try {
        final key = base64Decode(saved);
        if (key.length == keyLength) return key;
      } on FormatException {
        // Replaced below.
      }
    }
    final random = Random.secure();
    final key = Uint8List.fromList([
      for (var i = 0; i < keyLength; i++) random.nextInt(256),
    ]);
    await _storage.write(key: _name, value: base64Encode(key));
    return key;
  }
}
