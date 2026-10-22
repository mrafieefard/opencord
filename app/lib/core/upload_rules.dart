import 'package:opencord/core/model/attachment.dart';

const _reservedStems = {
  'con',
  'prn',
  'aux',
  'nul',
  'com1',
  'com2',
  'com3',
  'com4',
  'com5',
  'com6',
  'com7',
  'com8',
  'com9',
  'lpt1',
  'lpt2',
  'lpt3',
  'lpt4',
  'lpt5',
  'lpt6',
  'lpt7',
  'lpt8',
  'lpt9',
};

/// Computes the effective upload limit for a member in a server.
int effectiveUploadLimit({
  required bool isOwner,
  required int everyone,
  required Iterable<int?> roles,
  required int ceiling,
}) {
  if (isOwner) return ceiling;
  var maxLimit = everyone;
  for (final role in roles) {
    if (role != null && role > maxLimit) {
      maxLimit = role;
    }
  }
  return maxLimit < ceiling ? maxLimit : ceiling;
}

/// Sanitizes an uploaded filename according to the shared rules.
String sanitizeFilename(String input) {
  // 1. Keep only the part after the last / or \
  var name = input.split(RegExp(r'[/\\]')).last;

  // 2. Drop control characters: Rust's `char::is_control` is U+0000–U+001F
  // and U+007F–U+009F.
  name = String.fromCharCodes(
    name.runes.where((c) => c >= 0x20 && (c < 0x7F || c > 0x9F)),
  );

  // 3. Trim trailing . and spaces, and leading spaces
  name = name.replaceFirst(RegExp(r'^ +'), '');
  name = name.replaceFirst(RegExp(r'[. ]+$'), '');

  // 4. Empty -> "file"
  if (name.isEmpty) {
    return 'file';
  }

  // 5. Reserved DOS device names
  final dotIndex = name.indexOf('.');
  final stem = dotIndex == -1 ? name : name.substring(0, dotIndex);
  if (_reservedStems.contains(stem.toLowerCase())) {
    name = '_$name';
  }

  // 6. Length > 200 chars: cut the stem and keep the extension (<= 20 chars).
  // Counted in Unicode scalar values, as Rust's `chars()` does.
  final chars = name.runes.toList();
  if (chars.length > 200) {
    final lastDot = name.lastIndexOf('.');
    final ext = lastDot == -1
        ? null
        : name.substring(lastDot + 1).runes.toList();
    if (ext != null && ext.length <= 20) {
      final stem = name
          .substring(0, lastDot)
          .runes
          .take(200 - (ext.length + 1));
      name = '${String.fromCharCodes(stem)}.${String.fromCharCodes(ext)}';
    } else {
      name = String.fromCharCodes(chars.take(200));
    }
  }

  if (name.isEmpty) {
    return 'file';
  }

  return name;
}

/// Normalizes an extension: trim, strip one leading dot, lowercase.
String? normalizeExtension(String input) {
  var s = input.trim();
  if (s.startsWith('.')) {
    s = s.substring(1);
  }
  final lower = s.toLowerCase();
  if (lower.isEmpty || lower.length > 20) {
    return null;
  }
  if (lower.startsWith('.') || lower.endsWith('.') || lower.contains('..')) {
    return null;
  }
  for (final c in lower.codeUnits) {
    final isLower = c >= 97 && c <= 122; // a-z
    final isDigit = c >= 48 && c <= 57; // 0-9
    final isDot = c == 46; // .
    if (!isLower && !isDigit && !isDot) {
      return null;
    }
  }
  return lower;
}

/// Whether a sanitized filename ends with "." and a blocked extension.
bool isBlocked(String sanitizedFilename, List<String> blocked) =>
    blockedExtension(sanitizedFilename, blocked) != null;

/// The blocked extension a sanitized filename ends with, or null.
String? blockedExtension(String sanitizedFilename, List<String> blocked) {
  final lower = sanitizedFilename.toLowerCase();
  for (final entry in blocked) {
    final norm = entry.toLowerCase();
    if (lower.endsWith('.$norm')) return norm;
  }
  return null;
}

/// Validates whether a file can be staged in the composer.
StageProblem? stageProblem({
  required String name,
  required int size,
  required int limit,
  required List<String> blocked,
}) {
  if (size > limit) return StageProblem.tooLarge;
  if (isBlocked(sanitizeFilename(name), blocked)) return StageProblem.blocked;
  return null;
}

/// The "Block programs" preset: a copy of `opencord_common::upload::BLOCK_PROGRAMS`.
const blockProgramsPreset = [
  'exe',
  'msi',
  'msix',
  'appx',
  'bat',
  'cmd',
  'com',
  'scr',
  'pif',
  'cpl',
  'ps1',
  'psm1',
  'vbs',
  'vbe',
  'js',
  'jse',
  'wsf',
  'wsh',
  'hta',
  'lnk',
  'reg',
  'jar',
  'apk',
  'app',
  'dmg',
  'pkg',
  'deb',
  'rpm',
  'appimage',
  'sh',
  'run',
];
