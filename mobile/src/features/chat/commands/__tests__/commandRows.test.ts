import type { ChannelAccess, CommandListRecord, CommandRecord } from '@opencord/core-native';

import { fullAccess } from '@/core/fake/world';

import { commandUnavailable, flatRows, groupCommands, headerIndex, type CommandContext } from '../commandRows';

const spec = (name: string, feature: string, more: Partial<CommandRecord> = {}): CommandRecord => ({
  name,
  description: 'd',
  options: [],
  feature,
  source: feature.startsWith('plugin:') ? feature : '',
  permissionSummary: 'Control playback',
  aliases: [],
  needs: [],
  enabled: true,
  channelIds: [],
  ...more,
});

const list = (...commands: CommandRecord[]): CommandListRecord => ({ commands, resultsPublic: true });

const context: CommandContext = {
  channelId: '10',
  access: [],
  pluginUsers: [
    { userId: '1', pluginId: 'dice', name: 'Dice Bot', avatarHash: 'abc' },
    { userId: '2', pluginId: 'tickets', name: 'Tickets', avatarHash: undefined },
  ],
};

describe('commandRows', () => {
  it('opencord_first_then_install_order', () => {
    const groups = groupCommands(list(spec('x', 'plugin:tickets'), spec('y', 'plugin:dice'), spec('p', 'music')), '', context);

    expect(groups.map((group) => group.source.name)).toEqual(['Opencord', 'Tickets', 'Dice Bot']);
  });

  it('the_availability_rules_match_desktops', () => {
    expect(commandUnavailable(spec('a', 'plugin:dice', { channelIds: ['99'] }), context)).toBe('Not in this channel');
    expect(commandUnavailable(spec('a', 'plugin:dice'), context)).toBeUndefined();
    const music = spec('a', 'music', { needs: ['musicControl'] });
    expect(commandUnavailable(music, context)).toBe('Join a voice channel first');
    const denied: ChannelAccess = { ...fullAccess('20'), musicControl: false };
    expect(commandUnavailable(music, { ...context, voiceChannelId: '20', access: [denied] })).toBe('Needs Control playback');
    expect(commandUnavailable(music, { ...context, voiceChannelId: '20', access: [fullAccess('20')] })).toBeUndefined();
  });

  it('filter_by_name_and_empty_groups_drop', () => {
    const groups = groupCommands(
      list(spec('roll', 'plugin:dice'), spec('close', 'plugin:tickets'), spec('np', 'music', { aliases: ['nowplaying'] })),
      'NO',
      context,
    );

    expect(groups.map((group) => group.source.name)).toEqual(['Opencord']);
  });

  it('a_plugin_with_no_user_row_shows_its_id', () => {
    expect(groupCommands(list(spec('x', 'plugin:gone')), '', context)[0]?.source.name).toBe('gone');
  });

  it('5000_rows_group_quickly', () => {
    const many: CommandRecord[] = [];
    for (let plugin = 0; plugin < 50; plugin++) {
      for (let command = 0; command < 100; command++) many.push(spec(`c${command}`, `plugin:p${plugin}`));
    }

    const started = performance.now();
    const groups = groupCommands(list(...many), '', context);
    const items = flatRows(groups);

    expect(performance.now() - started).toBeLessThan(200);
    expect(items).toHaveLength(5050);
    expect(headerIndex(items, 49)).toBe(5050 - 101);
  });
});
