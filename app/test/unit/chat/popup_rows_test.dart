import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/popup/popup_rows.dart';
import 'package:opencord/features/chat/suggestions.dart';

import '../../support/fixtures.dart';

CommandSuggestion _command(String name, String feature) => CommandSuggestion(
  CommandSpec(name: name, description: 'd', feature: feature),
);

const _row = 36.0;
const _header = 30.0;

void main() {
  final data = ServerData.fromSnapshot(snapshot()).copyWith(
    pluginUsers: const {
      900: PluginUser(
        userId: 900,
        pluginId: 'dice',
        name: 'Dice Bot',
        avatarHash: 'abc',
      ),
      901: PluginUser(userId: 901, pluginId: 'tickets', name: 'Tickets'),
    },
  );

  test('opencord_comes_first_then_plugins_in_server_order', () {
    final groups = groupCommands([
      _command('close', 'plugin:tickets'),
      _command('roll', 'plugin:dice'),
      _command('play', 'music'),
      _command('volume', 'music'),
    ], data);

    expect(groups.map((g) => g.source.name), [
      'Opencord',
      'Tickets',
      'Dice Bot',
    ]);
    expect(groups.first.rows.map((r) => r.spec.name), ['play', 'volume']);
    expect(groups.map((g) => g.firstIndex), [0, 2, 3]);
  });

  test('the_flat_order_matches_the_groups', () {
    final groups = groupCommands([
      _command('close', 'plugin:tickets'),
      _command('play', 'music'),
    ], data);

    expect(flatRows(groups).map((r) => r.spec.name), ['play', 'close']);
  });

  test('empty_groups_are_left_out', () {
    final groups = groupCommands([_command('roll', 'plugin:dice')], data);

    expect(groups.map((g) => g.source.name), ['Dice Bot']);
    expect(groupCommands(const [], data), isEmpty);
  });

  test('a_plugin_without_an_avatar_still_groups_under_its_name', () {
    final groups = groupCommands([_command('x', 'plugin:tickets')], data);

    expect(groups.single.source.name, 'Tickets');
    expect(groups.single.source.avatarHash, '');
    expect(groups.single.source.userId, 901);
  });

  test('a_plugin_with_no_user_row_shows_its_id', () {
    final groups = groupCommands([_command('x', 'plugin:gone')], data);

    expect(groups.single.source.name, 'gone');
    expect(groups.single.source.userId, isNull);
  });

  test('same_name_stays_in_both_groups', () {
    final groups = groupCommands([
      _command('roll', 'plugin:dice'),
      _command('roll', 'plugin:tickets'),
    ], data);

    expect(groups.map((g) => g.rows.single.spec.name), ['roll', 'roll']);
  });

  test('stepRow_wraps', () {
    expect(stepRow(0, 1, 3), 1);
    expect(stepRow(2, 1, 3), 0);
    expect(stepRow(0, -1, 3), 2);
    expect(stepRow(0, 1, 0), 0);
  });

  test('stepGroup_jumps_and_wraps', () {
    final groups = groupCommands([
      _command('a', 'music'),
      _command('b', 'music'),
      _command('c', 'plugin:dice'),
      _command('d', 'plugin:tickets'),
    ], data);

    expect(stepGroup(groups, 1, 1), 2);
    expect(stepGroup(groups, 2, 1), 3);
    expect(stepGroup(groups, 3, 1), 0);
    expect(stepGroup(groups, 0, -1), 3);
    expect(stepGroup(groups, 2, -1), 0);
  });

  test('offset_and_group_are_inverse_across_5000_rows', () {
    final rows = [
      for (var plugin = 0; plugin < 50; plugin++)
        for (var command = 0; command < 100; command++)
          _command('c$command', 'plugin:p$plugin'),
    ];
    final groups = groupCommands(rows, data);
    expect(groups.length, 50);

    for (final (index, group) in groups.indexed) {
      final top = offsetOfRow(
        groups,
        group.firstIndex,
        row: _row,
        header: _header,
      );
      expect(top, index * (_header + 100 * _row));
      expect(groupAtOffset(groups, top, row: _row, header: _header), index);
      expect(
        groupAtOffset(
          groups,
          top + _header + 99 * _row,
          row: _row,
          header: _header,
        ),
        index,
      );
    }
  });

  test('a_later_row_sits_under_its_sticky_header', () {
    final groups = groupCommands([
      _command('a', 'music'),
      _command('b', 'music'),
      _command('c', 'plugin:dice'),
    ], data);

    expect(offsetOfRow(groups, 1, row: _row, header: _header), _row);
    expect(
      offsetOfRow(groups, 2, row: _row, header: _header),
      _header + 2 * _row,
    );
  });
}
