import {
  CommandRunStateRecord,
  ConnectionRecord,
  MobileEvent,
  MobileError,
  type EphemeralNoteRecord,
} from '@opencord/core-native';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import { dismissEphemeral, runCommand, suggestForOption } from '../actions/commands';
import { applyEvents } from '../events';
import { useAppStore } from '../store';
import type { AppState } from '../types';

const now = 1_761_500_000_000;

function ready(): AppState {
  freshStore();
  return applyEvents(useAppStore.getState(), [new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })], now);
}

const note = (state: CommandRunStateRecord, content: string, nonce = 'n1'): EphemeralNoteRecord => ({
  channelId: CHANNELS.general,
  nonce,
  content,
  state,
});

const received = (each: EphemeralNoteRecord) => new MobileEvent.EphemeralReceived({ serverKey: NIGHT_OWLS, note: each });

const notes = (state: AppState) => state.servers[NIGHT_OWLS]?.ephemeral?.[CHANNELS.general] ?? [];

describe('commands in the store', () => {
  it('commands_update_reaches_the_store', () => {
    const list = { commands: [], resultsPublic: false };

    const next = applyEvents(ready(), [new MobileEvent.CommandsUpdated({ serverKey: NIGHT_OWLS, commands: list })], now);

    expect(next.servers[NIGHT_OWLS]?.snapshot?.commands).toEqual(list);
  });

  it('an_ephemeral_with_the_same_nonce_replaces_the_note', () => {
    const next = applyEvents(
      ready(),
      [received(note(CommandRunStateRecord.Thinking, 'thinking')), received(note(CommandRunStateRecord.Answered, 'done'))],
      now,
    );

    expect(notes(next).map((each) => each.content)).toEqual(['done']);
  });

  it('notes_with_other_nonces_stay', () => {
    const next = applyEvents(
      ready(),
      [received(note(CommandRunStateRecord.Answered, 'one', 'a')), received(note(CommandRunStateRecord.Answered, 'two', 'b'))],
      now,
    );

    expect(notes(next).map((each) => each.content)).toEqual(['one', 'two']);
  });

  it('reconnecting_clears_thinking_notes', () => {
    const before = applyEvents(
      ready(),
      [received(note(CommandRunStateRecord.Thinking, 'thinking', 'a')), received(note(CommandRunStateRecord.Answered, 'kept', 'b'))],
      now,
    );

    const next = applyEvents(
      before,
      [new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() })],
      now,
    );

    expect(notes(next).map((each) => each.content)).toEqual(['kept']);
  });
});

describe('command actions', () => {
  beforeEach(async () => {
    useAppStore.setState(ready());
    await settle();
  });

  it('a_failed_run_becomes_an_only_you_note', async () => {
    jest
      .spyOn(getCore(), 'commandExecute')
      .mockRejectedValue(new MobileError.Server({ code: 'command_unknown', message: "/x isn't a command here", retryAfterMs: undefined }));

    await runCommand(NIGHT_OWLS, CHANNELS.general, 'x', '', []);

    const [only] = notes(useAppStore.getState());
    expect(only?.error).toBe(true);
    expect(only?.content).toContain("isn't a command here");
  });

  it('a_run_sends_its_source_and_a_nonce', async () => {
    const execute = jest.spyOn(getCore(), 'commandExecute');

    await runCommand(NIGHT_OWLS, CHANNELS.general, 'roll', 'plugin:dice', [{ name: 'sides', value: '6' }]);

    const call = execute.mock.calls[0];
    expect(call?.slice(0, 5)).toEqual([NIGHT_OWLS, CHANNELS.general, 'roll', 'plugin:dice', [{ name: 'sides', value: '6' }]]);
    expect(call?.[5]).toBeTruthy();
  });

  it('a_failing_suggestion_is_no_suggestions', async () => {
    jest.spyOn(getCore(), 'commandAutocomplete').mockRejectedValue(new MobileError.NotConnected());

    await expect(suggestForOption(NIGHT_OWLS, CHANNELS.general, 'x', '', 'o', 'v')).resolves.toEqual([]);
  });

  it('a_note_can_be_dismissed', async () => {
    useAppStore.setState(applyEvents(useAppStore.getState(), [received(note(CommandRunStateRecord.Answered, 'x'))], now));
    const [only] = notes(useAppStore.getState());

    dismissEphemeral(NIGHT_OWLS, CHANNELS.general, only!);

    expect(notes(useAppStore.getState())).toEqual([]);
  });
});
