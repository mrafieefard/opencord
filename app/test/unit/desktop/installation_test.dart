import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/installation.dart';

void main() {
  group('Installation.find', () {
    test('finds the install around a Linux or Windows app', () {
      final linux = Installation.find(
        '/home/a/.local/share/opencord/app-0.0.2-beta.3/opencord/opencord',
        {'OPENCORD_LAUNCHER': '/home/a/.local/share/opencord/opencord'},
      )!;
      expect(linux.version, '0.0.2-beta.3');
      expect(linux.launcher, '/home/a/.local/share/opencord/opencord');
      expect(
        linux.root.path.replaceAll(r'\', '/'),
        '/home/a/.local/share/opencord',
      );

      final windows = Installation.find(
        r'C:\Users\a\AppData\Local\Opencord\app-0.0.2\opencord\opencord.exe',
        {},
      )!;
      expect(windows.version, '0.0.2');
      expect(windows.launcher, isNull);
      expect(windows.root.path.replaceAll(r'\', '/'), contains('Opencord'));
    });

    test('finds the install around a Mac app', () {
      final mac = Installation.find(
        '/Users/a/Library/Application Support/Opencord/app-0.0.2-canary.1/'
        'Opencord.app/Contents/MacOS/Opencord',
        {'OPENCORD_LAUNCHER': ''},
        macOS: true,
      )!;
      expect(mac.version, '0.0.2-canary.1');
      expect(mac.launcher, isNull);
    });

    test('a build run from an archive or from source has none', () {
      expect(Installation.find('/home/a/opencord/opencord', {}), isNull);
      expect(Installation.find('/opt/app-/opencord/opencord', {}), isNull);
      expect(Installation.find('opencord', {}), isNull);
      expect(
        Installation.find(
          '/home/a/dev/opencord/app/build/macos/Opencord.app/Contents/MacOS/'
          'Opencord',
          {},
          macOS: true,
        ),
        isNull,
      );
    });
  });

  group('the track in update.json', () {
    late Directory root;
    late Installation installation;

    setUp(() {
      root = Directory.systemTemp.createTempSync('opencord-install');
      installation = Installation(root: root, version: '0.0.2', launcher: null);
    });

    tearDown(() => root.deleteSync(recursive: true));

    File settings() => File('${root.path}/update.json');

    test('reads what the installer wrote', () async {
      settings().writeAsStringSync('{"track":"canary"}');
      expect(await installation.readTrack(), ReleaseTrack.canary);
    });

    test('falls back when missing or broken', () async {
      expect(await installation.readTrack(), ReleaseTrack.stable);
      settings().writeAsStringSync('{broken');
      expect(
        await installation.readTrack(fallback: ReleaseTrack.beta),
        ReleaseTrack.beta,
      );
      settings().writeAsStringSync('{"track":"nightly"}');
      expect(await installation.readTrack(), ReleaseTrack.stable);
    });

    test('writing keeps what else the launcher keeps there', () async {
      settings().writeAsStringSync(
        '{"track":"canary","installer_track":"canary"}',
      );

      await installation.writeTrack(ReleaseTrack.beta);

      final written = jsonDecode(settings().readAsStringSync()) as Map;
      expect(written['track'], 'beta');
      expect(written['installer_track'], 'canary');
      expect(File('${settings().path}.partial').existsSync(), isFalse);
      expect(await installation.readTrack(), ReleaseTrack.beta);
    });
  });
}
