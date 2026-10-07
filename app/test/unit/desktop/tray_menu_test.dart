import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

void main() {
  test('the tray menu opens, mutes, deafens, sets status and quits (§15)', () {
    final menu = trayMenu(const TrayState());

    expect(
      [for (final item in menu) item.separator ? '—' : item.label],
      ['Open Opencord', '—', 'Mute', 'Deafen', 'Status', '—', 'Quit Opencord'],
    );
    expect(menu.first.action, const OpenWindow());
    expect(menu.last.action, const QuitApp());
  });

  test('mute and deafen show whether they are on', () {
    final menu = trayMenu(const TrayState(muted: true));
    final mute = menu.firstWhere((item) => item.label == 'Mute');
    final deafen = menu.firstWhere((item) => item.label == 'Deafen');

    expect(mute.checked, isTrue);
    expect(deafen.checked, isFalse);
    expect(mute.action, const ToggleMute());
    expect(deafen.action, const ToggleDeafen());
  });

  test('Status lists every presence with the current one chosen', () {
    final status = trayMenu(
      const TrayState(presence: SelfPresence.doNotDisturb),
    ).firstWhere((item) => item.label == 'Status');

    expect(status.children.map((item) => item.label), [
      'Online',
      'Idle',
      'Do not disturb',
      'Invisible',
    ]);
    expect(
      status.children.where((item) => item.checked == true).single.action,
      const SetPresence(SelfPresence.doNotDisturb),
    );
    expect(status.children.every((item) => item.radio), isTrue);
  });

  test('item ids are unique across the menu', () {
    final ids = <int>[];
    void collect(List<TrayMenuItem> items) {
      for (final item in items) {
        ids.add(item.id);
        collect(item.children);
      }
    }

    collect(trayMenu(const TrayState()));
    expect(ids.toSet(), hasLength(ids.length));
    expect(ids, isNot(contains(0)), reason: '0 is the root');
  });

  test('the tooltip counts unread messages and mentions', () {
    expect(const TrayState().tooltip, 'No unread messages');
    expect(const TrayState(unread: 3).tooltip, '3 unread messages');
    expect(
      const TrayState(unread: 1, mentions: 1).tooltip,
      '1 unread message, 1 mention',
    );
  });
}
