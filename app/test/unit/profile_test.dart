import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/profile.dart';

void main() {
  test('no OPENCORD_PROFILE is the default profile', () {
    expect(appProfile(const {}), '');
    expect(appProfile(const {'OPENCORD_PROFILE': '  '}), '');
  });

  test('a profile keeps letters, digits, - and _', () {
    expect(appProfile(const {'OPENCORD_PROFILE': 'bob'}), 'bob');
    expect(
      appProfile(const {'OPENCORD_PROFILE': 'Test User 2/x'}),
      'Test_User_2_x',
    );
  });

  test('the mock runs with OPENCORD_MOCK=1 or --mock', () {
    expect(usesMock(const [], const {}), isFalse);
    expect(usesMock(const ['--mock'], const {}), isTrue);
    expect(usesMock(const [], const {'OPENCORD_MOCK': '1'}), isTrue);
    expect(usesMock(const [], const {'OPENCORD_MOCK': '0'}), isFalse);
  });
}
