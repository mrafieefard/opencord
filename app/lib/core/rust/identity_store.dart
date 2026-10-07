import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'package:opencord/core/repository/repository.dart';

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

  // macOS: the login keychain. The data protection keychain the plugin
  // defaults to needs a keychain-access-groups entitlement and a
  // provisioning profile, and without them nothing is saved.
  final _storage = const FlutterSecureStorage(
    mOptions: MacOsOptions(usesDataProtectionKeychain: false),
  );

  String get _secretKey => '${_prefix}identity.secret';
  String get _nameKey => '${_prefix}identity.name';

  /// Runs a keychain call, its failures as the app's errors: no Secret
  /// Service on the bus, a locked keyring whose prompt was dismissed.
  Future<T> _keychain<T>(Future<T> Function() call) async {
    try {
      return await call();
    } on PlatformException catch (error) {
      throw RepoException(
        RepoErrorKind.other,
        'Opencord keeps your identity in the system keyring, which could '
        'not be used (${error.message ?? error.code}).',
      );
    }
  }

  @override
  Future<SavedIdentity?> read() async {
    final secret = await _keychain(() => _storage.read(key: _secretKey));
    final name = await _keychain(() => _storage.read(key: _nameKey));
    if (secret == null || name == null) return null;
    try {
      return SavedIdentity(secret: base64Decode(secret), displayName: name);
    } on FormatException {
      return null;
    }
  }

  @override
  Future<void> write(SavedIdentity identity) async {
    await _keychain(
      () =>
          _storage.write(key: _secretKey, value: base64Encode(identity.secret)),
    );
    await _keychain(
      () => _storage.write(key: _nameKey, value: identity.displayName),
    );
  }

  /// Forgets this profile's identity, and only it: on Linux every key of
  /// the app shares one keychain item.
  Future<void> clear() async {
    await _keychain(() => _storage.delete(key: _secretKey));
    await _keychain(() => _storage.delete(key: _nameKey));
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
