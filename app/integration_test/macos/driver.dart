// Runs a Mac integration test with `flutter drive`, which can build in
// profile mode (optimized Rust and Dart) where `flutter test` can't: for
// the CPU numbers in docs/plans/macos.md §7.
//
//   flutter drive --profile -d macos --driver=integration_test/macos/driver.dart \
//     --target=integration_test/macos/camera_test.dart
import 'package:integration_test/integration_test_driver.dart';

Future<void> main() => integrationDriver();
