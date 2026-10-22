// Runs an integration test through `flutter drive`, for profile builds
// (tool/check_v5_load.sh): what the test reports lands in
// build/integration_response_data.json.
import 'package:integration_test/integration_test_driver.dart';

Future<void> main() => integrationDriver();
