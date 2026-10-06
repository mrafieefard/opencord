import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/app.dart';
import 'package:opencord/src/rust/api/system.dart';
import 'package:opencord/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('placeholder shows the version reported by the Rust core', (
    tester,
  ) async {
    await tester.pumpWidget(OpencordApp(coreVersion: coreVersion()));

    expect(
      find.textContaining(RegExp(r'^Core \d+\.\d+\.\d+ \(protocol v\d+\)$')),
      findsOneWidget,
    );
  });
}
