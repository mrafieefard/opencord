import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/upload_rules.dart';

void main() {
  final fixtureFile = File('../crates/opencord-common/tests/upload_rules.json');
  final json =
      jsonDecode(fixtureFile.readAsStringSync()) as Map<String, dynamic>;

  group('effectiveUploadLimit', () {
    for (final caseData in json['limits'] as List) {
      final owner = caseData['owner'] as bool;
      final everyone = caseData['everyone'] as int;
      final roles = (caseData['roles'] as List).map(
        (r) => r == null ? null : (r as num).toInt(),
      );
      final ceiling = caseData['ceiling'] as int;
      final expected = caseData['expected'] as int;

      test('limit for owner: $owner, everyone: $everyone', () {
        final actual = effectiveUploadLimit(
          isOwner: owner,
          everyone: everyone,
          roles: roles,
          ceiling: ceiling,
        );
        expect(actual, expected);
      });
    }
  });

  group('sanitizeFilename', () {
    for (final caseData in json['filenames'] as List) {
      final input = caseData['input'] as String;
      final expected = caseData['expected'] as String;

      test('sanitize $input -> $expected', () {
        expect(sanitizeFilename(input), expected);
      });
    }

    test('long filename is cut to 200 chars keeping extension', () {
      final input = '${'a' * 296}.png';
      final sanitized = sanitizeFilename(input);
      expect(sanitized.length, 200);
      expect(sanitized.endsWith('.png'), isTrue);
    });
  });

  group('normalizeExtension', () {
    for (final caseData in json['extensions'] as List) {
      final input = caseData['input'] as String;
      final expected = caseData['expected'] as String?;

      test('normalize $input -> $expected', () {
        expect(normalizeExtension(input), expected);
      });
    }
  });

  group('isBlocked', () {
    for (final caseData in json['blocked'] as List) {
      final filename = caseData['filename'] as String;
      final list = (caseData['list'] as List).cast<String>();
      final expected = caseData['expected'] as bool;

      test('$filename against $list -> $expected', () {
        expect(isBlocked(filename, list), expected);
      });
    }
  });

  group('stageProblem', () {
    test('detects tooLarge', () {
      expect(
        stageProblem(
          name: 'large.zip',
          size: 60 * 1024 * 1024,
          limit: 50 * 1024 * 1024,
          blocked: [],
        ),
        StageProblem.tooLarge,
      );
    });

    test('detects blocked extension', () {
      expect(
        stageProblem(
          name: 'setup.exe',
          size: 1024,
          limit: 50 * 1024 * 1024,
          blocked: ['exe'],
        ),
        StageProblem.blocked,
      );
    });

    test('returns null when valid', () {
      expect(
        stageProblem(
          name: 'photo.jpg',
          size: 1024,
          limit: 50 * 1024 * 1024,
          blocked: ['exe'],
        ),
        isNull,
      );
    });
  });
}
