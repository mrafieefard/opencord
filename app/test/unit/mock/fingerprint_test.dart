import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_world.dart';

void main() {
  test('fake fingerprints depend on the whole seed', () {
    expect(
      fakeFingerprint('opencord.example:7710'),
      isNot(fakeFingerprint('opencord.example:7710-impostor')),
    );
    expect(
      fakeFingerprint('a.example:1'),
      isNot(fakeFingerprint('a.example:2')),
    );
  });

  test('fake fingerprints look like SHA-256 hex', () {
    expect(fakeFingerprint('x'), matches(RegExp(r'^[0-9a-f]{64}$')));
    expect(fakeFingerprint('x'), fakeFingerprint('x'));
  });
}
