import { act, renderHook, waitFor } from '@testing-library/react-native';
import { CommandOptionKindRecord, ConnectionRecord, MobileError, type CommandOptionRecord, type CommandRecord } from '@opencord/core-native';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { EMPTY_MARK, useCommandDraft } from '../useCommandDraft';

const option = (name: string, more: Partial<CommandOptionRecord> = {}): CommandOptionRecord => ({
  name,
  description: `About ${name}`,
  kind: CommandOptionKindRecord.Text,
  required: false,
  autocomplete: false,
  choices: [],
  min: 0,
  max: 0,
  ...more,
});

const spec = (options: CommandOptionRecord[]): CommandRecord => ({
  name: 'ticket',
  description: 'Open one',
  options,
  feature: 'plugin:tickets',
  source: 'plugin:tickets',
  permissionSummary: '',
  aliases: [],
  needs: [],
  enabled: true,
  channelIds: [],
});

const hook = () => renderHook(() => useCommandDraft(NIGHT_OWLS, CHANNELS.general));

describe('useCommandDraft', () => {
  beforeEach(async () => {
    freshStore();
    await settle();
    useAppStore.setState((state) => ({
      servers: {
        ...state.servers,
        [NIGHT_OWLS]: {
          key: NIGHT_OWLS,
          entry: { key: NIGHT_OWLS, name: 'Night Owls', host: 'h', port: 1 },
          connection: new ConnectionRecord.Connected(),
          snapshot: snapshot(NIGHT_OWLS),
          seeding: false,
          readyCount: 1,
          held: [],
          activity: {},
          typing: {},
          voice: {},
          nowPlaying: {},
          musicSettings: null,
        },
      },
    }));
  });
  afterEach(() => jest.restoreAllMocks());

  it('picking_makes_a_chip_and_option_chips', async () => {
    const { result } = await hook();

    await act(async () => result.current.pick(spec([option('topic'), option('note')])));

    expect(result.current.spec?.name).toBe('ticket');
    expect(result.current.active?.name).toBe('topic');
  });

  it('the_empty_field_holds_the_mark', async () => {
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('topic')])));

    expect(result.current.fieldValue).toBe(EMPTY_MARK);
    expect(result.current.fieldText).toBe('');
  });

  it('backspace_on_empty_removes_the_chip', async () => {
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('topic')])));

    await act(async () => result.current.changeField(''));

    expect(result.current.spec).toBeNull();
  });

  it('backspace_is_seen_through_the_text_change_alone', async () => {
    const { result } = await hook();
    await act(async () => result.current.pick(spec([])));
    expect(result.current.fieldValue).toBe(EMPTY_MARK);

    await act(async () => result.current.changeField(''));

    expect(result.current.spec).toBeNull();
  });

  it('backspace_on_a_later_empty_option_steps_back_to_the_one_before', async () => {
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('topic'), option('note')])));
    await act(async () => result.current.focusOption('note'));

    await act(async () => result.current.changeField(''));

    expect(result.current.spec).not.toBeNull();
    expect(result.current.active?.name).toBe('topic');
  });

  it('deleting_the_last_letter_is_not_a_backspace_out', async () => {
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('topic')])));
    await act(async () => result.current.changeField(`${EMPTY_MARK}a`));
    expect(result.current.fieldText).toBe('a');

    await act(async () => result.current.changeField(''));

    expect(result.current.spec).not.toBeNull();
    expect(result.current.fieldValue).toBe(EMPTY_MARK);
  });

  it('the_empty_mark_is_never_sent_or_shown', async () => {
    const execute = jest.spyOn(getCore(), 'commandExecute');
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('topic')])));
    await act(async () => result.current.changeField(`${EMPTY_MARK}billing`));

    await act(async () => {
      await result.current.run();
    });

    expect(execute.mock.calls[0]?.[4]).toEqual([{ name: 'topic', value: 'billing' }]);
  });

  it('send_runs_with_source_and_nonce', async () => {
    const execute = jest.spyOn(getCore(), 'commandExecute');
    const { result } = await hook();
    await act(async () => result.current.pick(spec([])));

    await act(async () => {
      await result.current.run();
    });

    const call = execute.mock.calls[0];
    expect(call?.slice(0, 4)).toEqual([NIGHT_OWLS, CHANNELS.general, 'ticket', 'plugin:tickets']);
    expect(call?.[5]).toBeTruthy();
    expect(result.current.spec).toBeNull();
  });

  it('a_missing_required_option_focuses_it_and_does_not_send', async () => {
    const execute = jest.spyOn(getCore(), 'commandExecute');
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('note'), option('topic', { required: true })])));

    await act(async () => {
      await result.current.run();
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.current.active?.name).toBe('topic');
    expect(result.current.missing?.name).toBe('topic');
  });

  it('a_removed_command_gives_a_plain_note_and_keeps_the_draft', async () => {
    jest
      .spyOn(getCore(), 'commandExecute')
      .mockRejectedValue(new MobileError.Server({ code: 'command_unknown', message: "/ticket isn't a command here", retryAfterMs: undefined }));
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('topic')])));
    await act(async () => result.current.changeField('billing'));

    await act(async () => {
      await result.current.run();
    });

    expect(result.current.spec?.name).toBe('ticket');
    expect(result.current.fieldText).toBe('billing');
    const notes = useAppStore.getState().servers[NIGHT_OWLS]?.ephemeral?.[CHANNELS.general] ?? [];
    expect(notes[0]?.error).toBe(true);
  });

  it('choice_boolean_and_autocomplete_open_their_list_in_the_same_spot', async () => {
    const choice = option('size', {
      kind: CommandOptionKindRecord.Choice,
      choices: [{ name: 'Small', value: 's', artworkId: '' }],
    });
    const yes = option('on', { kind: CommandOptionKindRecord.Boolean });
    const auto = option('song', { autocomplete: true });
    const { result } = await hook();

    await act(async () => result.current.pick(spec([choice, yes, auto])));
    expect(result.current.choiceRows.map((row) => row.key)).toEqual(['s']);

    await act(async () => result.current.focusOption('on'));
    expect(result.current.choiceRows.map((row) => row.key)).toEqual(['yes', 'no']);

    await act(async () => result.current.focusOption('song'));
    await act(async () => result.current.changeField('mi'));
    await waitFor(() => expect(result.current.choiceRows.length).toBe(2));
  });

  it('picking_a_choice_shows_its_name_and_sends_its_value', async () => {
    const execute = jest.spyOn(getCore(), 'commandExecute');
    const choice = option('size', {
      kind: CommandOptionKindRecord.Choice,
      choices: [{ name: 'Small', value: 's', artworkId: '' }],
    });
    const { result } = await hook();
    await act(async () => result.current.pick(spec([choice])));

    await act(async () => {
      const [row] = result.current.choiceRows;
      if (row) result.current.pickChoice(row);
    });
    expect(result.current.fieldText).toBe('Small');
    expect(result.current.choiceRows).toEqual([]);

    await act(async () => {
      await result.current.run();
    });
    expect(execute.mock.calls[0]?.[4]).toEqual([{ name: 'size', value: 's' }]);
  });

  it('outdated_suggestions_are_dropped', async () => {
    const answers: ((value: { name: string; value: string; artworkId: string }[]) => void)[] = [];
    jest
      .spyOn(getCore(), 'commandAutocomplete')
      .mockImplementation(() => new Promise((resolve) => answers.push(resolve)));
    const { result } = await hook();
    await act(async () => result.current.pick(spec([option('song', { autocomplete: true })])));

    await act(async () => result.current.changeField('ab'));
    await waitFor(() => expect(answers.length).toBe(1));
    await act(async () => result.current.changeField('abc'));
    await waitFor(() => expect(answers.length).toBe(2));

    await act(async () => answers[1]?.([{ name: 'new', value: 'n', artworkId: '' }]));
    await act(async () => answers[0]?.([{ name: 'old', value: 'o', artworkId: '' }]));

    expect(result.current.choiceRows.map((row) => row.key)).toEqual(['n']);
  });
});
