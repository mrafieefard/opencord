import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';

const _plugin = MethodChannel('plugins.it_nomads.com/flutter_secure_storage');

final _identity = SavedIdentity(
  secret: Uint8List.fromList([1, 2, 3]),
  displayName: 'Alex',
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final calls = <MethodCall>[];

  void keychain(Object? Function(MethodCall call) answer) =>
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(_plugin, (call) async {
            calls.add(call);
            return answer(call);
          });

  tearDown(() {
    calls.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_plugin, null);
  });

  test('a keyring that cannot be used says so as a repository error', () async {
    keychain(
      (_) => throw PlatformException(
        code: 'Libsecret error',
        message: 'Failed to unlock the keyring',
      ),
    );
    final store = SecureIdentityStore();

    await expectLater(
      store.read(),
      throwsA(
        isA<RepoException>().having(
          (e) => e.message,
          'message',
          contains('Failed to unlock the keyring'),
        ),
      ),
    );
    await expectLater(store.write(_identity), throwsA(isA<RepoException>()));
  });

  test(
    'macOS keeps it in the login keychain, which needs no entitlement',
    () async {
      keychain((_) => null);
      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      try {
        await SecureIdentityStore().write(_identity);
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }

      final options = (calls.first.arguments as Map)['options'] as Map;
      expect(options['usesDataProtectionKeychain'], 'false');
    },
  );
}
