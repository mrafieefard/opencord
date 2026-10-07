import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/login_item.dart';

void main() {
  late Directory config;

  setUp(() => config = Directory.systemTemp.createTempSync('opencord-xdg'));
  tearDown(() => config.deleteSync(recursive: true));

  File entry() =>
      File('${config.path}/autostart/dev.opencord.opencord.desktop');

  test('turning it on writes an XDG autostart entry (§15)', () async {
    final autostart = XdgAutostart(
      configHome: config,
      executable: '/opt/opencord/opencord',
    );

    await autostart.setEnabled(true);

    final text = entry().readAsStringSync();
    expect(text, startsWith('[Desktop Entry]\n'));
    expect(text, contains('\nType=Application\n'));
    expect(text, contains('\nName=Opencord\n'));
    // Marked as a start at login: only then does Start minimized apply.
    expect(text, contains('\nExec="/opt/opencord/opencord" --autostart\n'));
  });

  test(
    'turning it off removes the entry, and is fine when there is none',
    () async {
      final autostart = XdgAutostart(
        configHome: config,
        executable: '/opt/opencord/opencord',
      );
      await autostart.setEnabled(true);

      await autostart.setEnabled(false);
      await autostart.setEnabled(false);

      expect(entry().existsSync(), isFalse);
    },
  );

  test('the program path is quoted as desktop entries want', () async {
    final autostart = XdgAutostart(
      configHome: config,
      executable: r'/home/a b/my "app"/$bin',
    );

    await autostart.setEnabled(true);

    expect(
      entry().readAsStringSync(),
      contains(r'Exec="/home/a b/my \"app\"/\$bin" --autostart'),
    );
  });

  test('the config home follows XDG_CONFIG_HOME, else ~/.config', () {
    expect(
      xdgConfigHome({'XDG_CONFIG_HOME': '/x/config', 'HOME': '/home/a'}).path,
      '/x/config',
    );
    expect(xdgConfigHome({'HOME': '/home/a'}).path, '/home/a/.config');
    expect(
      xdgConfigHome({'XDG_CONFIG_HOME': '', 'HOME': '/home/a'}).path,
      '/home/a/.config',
    );
  });
}
