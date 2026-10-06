import 'package:flutter/material.dart';

import 'app.dart';
import 'src/rust/api/system.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(OpencordApp(coreVersion: coreVersion()));
}
