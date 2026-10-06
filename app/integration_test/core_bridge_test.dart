import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/src/rust/api/system.dart';
import 'package:opencord/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  test('the Rust core reports its version', () {
    expect(
      coreVersion(),
      matches(RegExp(r'^\d+\.\d+\.\d+ \(protocol v\d+\)$')),
    );
  });
}
