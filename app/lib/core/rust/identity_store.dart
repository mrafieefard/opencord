import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

/// The identity this device uses: its secret key and display name (Phase 1
/// §9.1).
@immutable
class SavedIdentity {
  const SavedIdentity({required this.secret, required this.displayName});

  final Uint8List secret;
  final String displayName;
}

/// Where the identity lives between runs.
abstract interface class IdentityStore {
  Future<SavedIdentity?> read();

  Future<void> write(SavedIdentity identity);
}

/// The system keychain through flutter_secure_storage (the Secret Service
/// on Linux, the Keychain on macOS, Credential Manager on Windows).
/// [profile] keeps a second instance's identity apart, for testing two
/// users on one machine.
class SecureIdentityStore implements IdentityStore {
  SecureIdentityStore({String profile = ''})
    : _prefix = profile.isEmpty ? 'opencord.' : 'opencord.$profile.';

  final String _prefix;
  final _storage = const FlutterSecureStorage();

  String get _secretKey => '${_prefix}identity.secret';
  String get _nameKey => '${_prefix}identity.name';

  @override
  Future<SavedIdentity?> read() async {
    final secret = await _storage.read(key: _secretKey);
    final name = await _storage.read(key: _nameKey);
    if (secret == null || name == null) return null;
    try {
      return SavedIdentity(secret: base64Decode(secret), displayName: name);
    } on FormatException {
      return null;
    }
  }

  @override
  Future<void> write(SavedIdentity identity) async {
    await _storage.write(key: _secretKey, value: base64Encode(identity.secret));
    await _storage.write(key: _nameKey, value: identity.displayName);
  }
}

/// For tests.
class MemoryIdentityStore implements IdentityStore {
  MemoryIdentityStore([this.saved]);

  SavedIdentity? saved;

  @override
  Future<SavedIdentity?> read() async => saved;

  @override
  Future<void> write(SavedIdentity identity) async => saved = identity;
}
