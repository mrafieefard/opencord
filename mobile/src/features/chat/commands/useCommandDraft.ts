// A command being written on the phone: its options' values, which option
// the field edits, and what Backspace does. Backspace on an empty field
// can't be asked of every Android keyboard, so an empty field holds one
// invisible character and losing it is the Backspace.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  CommandOptionKindRecord,
  type CommandChoiceRecord,
  type CommandOptionRecord,
  type CommandRecord,
} from '@opencord/core-native';

import { runCommand, suggestForOption } from '@/state/actions/commands';

import type { CommandRow } from './commandRows';

export const EMPTY_MARK = '​';

/** How long typing must pause before suggestions are asked for. */
const SUGGEST_AFTER_MS = 400;
/** Fewer letters than this ask for nothing. */
const SUGGEST_MIN_LETTERS = 2;

export interface OptionValue {
  /** What's shown. */
  label: string;
  /** What's sent. */
  value: string;
}

const strip = (text: string) => text.split(EMPTY_MARK).join('');

const YES_NO: CommandChoiceRecord[] = [
  { name: 'Yes', value: 'yes', artworkId: '' },
  { name: 'No', value: 'no', artworkId: '' },
];

function choiceRow(choice: CommandChoiceRecord): CommandRow {
  return { kind: 'choice', key: choice.value, choice };
}

export function useCommandDraft(serverKey: string, channelId: string) {
  const [spec, setSpec] = useState<CommandRecord | null>(null);
  const [values, setValues] = useState<Record<string, OptionValue>>({});
  const [activeName, setActiveName] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<CommandChoiceRecord[]>([]);
  const [choicesClosed, setChoicesClosed] = useState(false);
  const [running, setRunning] = useState(false);
  const asked = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const active = useMemo(
    () => spec?.options.find((option) => option.name === activeName) ?? null,
    [spec, activeName],
  );
  const fieldText = active ? values[active.name]?.label ?? '' : '';
  const fieldValue = fieldText === '' ? EMPTY_MARK : fieldText;

  const missing = useMemo(
    () => spec?.options.find((option) => option.required && !values[option.name]) ?? null,
    [spec, values],
  );

  const stopAsking = useCallback(() => {
    asked.current += 1;
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }, []);

  useEffect(() => stopAsking, [stopAsking]);

  const moveTo = useCallback(
    (name: string | null) => {
      stopAsking();
      setActiveName(name);
      setSuggestions([]);
      setChoicesClosed(false);
    },
    [stopAsking],
  );

  const pick = useCallback(
    (next: CommandRecord) => {
      setSpec(next);
      setValues({});
      moveTo(next.options[0]?.name ?? null);
    },
    [moveTo],
  );

  const clear = useCallback(() => {
    setSpec(null);
    setValues({});
    moveTo(null);
  }, [moveTo]);

  const setValue = useCallback((name: string, value: OptionValue | null) => {
    setValues((current) => {
      const { [name]: _old, ...rest } = current;
      return value ? { ...rest, [name]: value } : rest;
    });
  }, []);

  const focusOption = useCallback((name: string) => moveTo(name), [moveTo]);

  /** On an empty field: back an option, or out of the command. */
  const backspace = useCallback(() => {
    if (!spec) return;
    const at = spec.options.findIndex((option) => option.name === activeName);
    const before = spec.options[at - 1];
    if (at > 0 && before) moveTo(before.name);
    else clear();
  }, [spec, activeName, moveTo, clear]);

  const askForSuggestions = useCallback(
    (command: CommandRecord, option: CommandOptionRecord, typed: string) => {
      stopAsking();
      setSuggestions([]);
      if (typed.trim().length < SUGGEST_MIN_LETTERS) return;
      const ask = asked.current;
      timer.current = setTimeout(() => {
        void suggestForOption(serverKey, channelId, command.name, command.source, option.name, typed).then((found) => {
          if (ask === asked.current) setSuggestions(found);
        });
      }, SUGGEST_AFTER_MS);
    },
    [serverKey, channelId, stopAsking],
  );

  /** The field changed: what's typed, or the mark gone from an empty field. */
  const changeField = useCallback(
    (next: string) => {
      if (!spec) return;
      const typed = strip(next);
      if (next === '' && fieldText === '') {
        backspace();
        return;
      }
      if (!active) return;
      setChoicesClosed(false);
      setValue(active.name, typed === '' ? null : { label: typed, value: typed });
      if (active.autocomplete) askForSuggestions(spec, active, typed);
    },
    [spec, fieldText, active, backspace, setValue, askForSuggestions],
  );

  const choiceRows = useMemo<CommandRow[]>(() => {
    if (!active || choicesClosed) return [];
    if (active.kind === CommandOptionKindRecord.Boolean) return YES_NO.map(choiceRow);
    if (active.kind === CommandOptionKindRecord.Choice) {
      const typed = fieldText.toLowerCase();
      return active.choices.filter((choice) => choice.name.toLowerCase().includes(typed)).map(choiceRow);
    }
    return active.autocomplete ? suggestions.map(choiceRow) : [];
  }, [active, choicesClosed, fieldText, suggestions]);

  const pickChoice = useCallback(
    (row: CommandRow) => {
      if (row.kind !== 'choice' || !active) return;
      stopAsking();
      setValue(active.name, { label: row.choice.name, value: row.choice.value });
      setChoicesClosed(true);
    },
    [active, setValue, stopAsking],
  );

  const closeChoices = useCallback(() => setChoicesClosed(true), []);

  /** Sends it. What's missing is focused; a refusal keeps what was written. */
  const run = useCallback(async () => {
    if (!spec || running) return;
    if (missing) {
      moveTo(missing.name);
      return;
    }
    setRunning(true);
    const options = Object.entries(values).map(([name, each]) => ({ name, value: each.value }));
    const ran = await runCommand(serverKey, channelId, spec.name, spec.source, options);
    setRunning(false);
    if (ran) clear();
  }, [spec, running, missing, moveTo, values, serverKey, channelId, clear]);

  return {
    spec,
    source: spec?.source ?? '',
    values,
    active,
    running,
    fieldText,
    fieldValue,
    missing,
    choiceRows,
    pick,
    setValue,
    focusOption,
    clear,
    changeField,
    pickChoice,
    closeChoices,
    run,
  };
}
