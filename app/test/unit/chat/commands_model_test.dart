import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/commands.dart';

const _music = CommandSpec(name: 'play', description: 'd', feature: 'music');
const _diceRoll = CommandSpec(
  name: 'roll',
  description: 'd',
  feature: 'plugin:dice',
);
const _ticketsRoll = CommandSpec(
  name: 'roll',
  description: 'd',
  feature: 'plugin:tickets',
);
const _ticketsPlay = CommandSpec(
  name: 'play',
  description: 'd',
  feature: 'plugin:tickets',
);

void main() {
  test('source_is_the_plugin_feature_or_empty', () {
    expect(_music.source, '');
    expect(_music.pluginId, isNull);
    expect(_diceRoll.source, 'plugin:dice');
    expect(_diceRoll.pluginId, 'dice');
  });

  test('find_prefers_opencord_without_a_source', () {
    const list = CommandList(commands: [_ticketsPlay, _music]);

    expect(list.find('play'), _music);
  });

  test('find_without_a_source_takes_the_only_plugin', () {
    const list = CommandList(commands: [_diceRoll]);

    expect(list.find('roll'), _diceRoll);
  });

  test('find_without_a_source_never_guesses_between_two_plugins', () {
    const list = CommandList(commands: [_diceRoll, _ticketsRoll]);

    expect(list.find('roll'), isNull);
  });

  test('find_with_a_source_finds_only_that_one', () {
    const list = CommandList(commands: [_diceRoll, _ticketsRoll, _music]);

    expect(list.find('roll', source: 'plugin:tickets'), _ticketsRoll);
    expect(list.find('play', source: 'plugin:tickets'), isNull);
  });
}
