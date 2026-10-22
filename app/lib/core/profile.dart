/// A second copy of the app for testing two users on one machine
/// (OPENCORD_PROFILE): its own data, keychain entries and single-instance
/// name. Empty for the normal app. Characters other than letters, digits,
/// `-` and `_` become `_`.
String appProfile(Map<String, String> environment) =>
    (environment['OPENCORD_PROFILE'] ?? '').trim().replaceAll(
      RegExp(r'[^A-Za-z0-9_-]'),
      '_',
    );

/// Whether to run on the mock repository instead of the Rust core, for
/// working on the UI without a server: `OPENCORD_MOCK=1` or `--mock`.
bool usesMock(List<String> args, Map<String, String> environment) =>
    args.contains('--mock') || environment['OPENCORD_MOCK'] == '1';
