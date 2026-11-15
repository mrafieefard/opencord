// Slash commands: running one, an option's suggestions, and the replies
// only this device sees.
import type { CommandChoiceRecord, CommandOptionValueRecord } from '@opencord/core-native';
import { CommandRunStateRecord, MobileError } from '@opencord/core-native';

import { describeError, getCore } from '@/core';
import { withNote } from '@/state/events';
import { useAppStore } from '@/state/store';
import type { EphemeralNote } from '@/state/types';

/** The server's own words: commands' refusals are written for the member. */
function refusal(error: unknown): string {
  return MobileError.Server.instanceOf(error) ? error.inner.message : describeError(error);
}

function addNote(serverKey: string, note: EphemeralNote): void {
  useAppStore.setState((state) => {
    const server = state.servers[serverKey];
    return server ? { servers: { ...state.servers, [serverKey]: withNote(server, note) } } : state;
  });
}

/** Runs a command; false if it was refused, which becomes a note only you see. */
export async function runCommand(
  serverKey: string,
  channelId: string,
  name: string,
  source: string,
  options: CommandOptionValueRecord[],
): Promise<boolean> {
  const nonce = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  try {
    await getCore().commandExecute(serverKey, channelId, name, source, options, nonce);
    return true;
  } catch (error: unknown) {
    addNote(serverKey, {
      channelId,
      nonce,
      content: refusal(error),
      state: CommandRunStateRecord.Answered,
      error: true,
    });
    return false;
  }
}

/** An option's suggestions. Failing just offers none. */
export async function suggestForOption(
  serverKey: string,
  channelId: string,
  name: string,
  source: string,
  option: string,
  value: string,
): Promise<CommandChoiceRecord[]> {
  try {
    return await getCore().commandAutocomplete(serverKey, channelId, name, source, option, value);
  } catch {
    return [];
  }
}

export function dismissEphemeral(serverKey: string, channelId: string, note: EphemeralNote): void {
  useAppStore.setState((state) => {
    const server = state.servers[serverKey];
    if (!server) return state;
    const notes = (server.ephemeral?.[channelId] ?? []).filter((each) => each !== note);
    return {
      servers: { ...state.servers, [serverKey]: { ...server, ephemeral: { ...server.ephemeral, [channelId]: notes } } },
    };
  });
}
