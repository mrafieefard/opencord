import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Adds the licenses of the bundled fonts and the emoji list to the app's
/// license page.
void registerFontLicenses() {
  LicenseRegistry.addLicense(() async* {
    for (final (package, file) in const [
      ('Inter', 'Inter-OFL.txt'),
      ('JetBrains Mono', 'JetBrainsMono-OFL.txt'),
      ('Material Symbols', 'MaterialSymbols-Apache-2.0.txt'),
      ('emoji-data', 'emoji-data-MIT.txt'),
    ]) {
      final text = await rootBundle.loadString('assets/licenses/$file');
      yield LicenseEntryWithLineBreaks([package], text);
    }
  });
}
