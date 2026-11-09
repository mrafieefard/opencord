// The command list without a screen: who a command comes from, whether it
// can be used here, and the groups the list shows. The same rules as the
// desktop's (command_input.dart's commandUnavailable).
import type {
  ChannelAccess,
  CommandChoiceRecord,
  CommandListRecord,
  CommandRecord,
  PluginUserRecord,
} from '@opencord/core-native';

export interface CommandSource {
  /** Empty for Opencord, `plugin:<id>` for a plugin. */
  id: string;
  name: string;
  avatarHash?: string;
}

export type CommandRow =
  | { kind: 'command'; key: string; spec: CommandRecord; source: CommandSource; unavailable?: string }
  | { kind: 'choice'; key: string; choice: CommandChoiceRecord };

export interface CommandGroup {
  source: CommandSource;
  rows: CommandRow[];
}

export interface CommandContext {
  channelId: string;
  access: readonly ChannelAccess[];
  /** The voice channel this device is in on this server, if any. */
  voiceChannelId?: string;
  pluginUsers: readonly PluginUserRecord[];
}

const PLUGIN_PREFIX = 'plugin:';
const OPENCORD: CommandSource = { id: '', name: 'Opencord' };

/** Why a command can't be used here now; undefined when it can. */
export function commandUnavailable(spec: CommandRecord, context: CommandContext): string | undefined {
  if (spec.channelIds.length > 0 && !spec.channelIds.includes(context.channelId)) return 'Not in this channel';
  if (spec.feature !== 'music') return undefined;
  if (context.voiceChannelId === undefined) return 'Join a voice channel first';
  const access = context.access.find((each) => each.channelId === context.voiceChannelId) as
    | Record<string, unknown>
    | undefined;
  return spec.needs.every((need) => access?.[need] === true) ? undefined : `Needs ${spec.permissionSummary}`;
}

function sourceOf(id: string, context: CommandContext): CommandSource {
  if (id === '') return OPENCORD;
  const pluginId = id.slice(PLUGIN_PREFIX.length);
  const user = context.pluginUsers.find((each) => each.pluginId === pluginId);
  return { id, name: user?.name ?? pluginId, avatarHash: user?.avatarHash };
}

function matches(spec: CommandRecord, query: string): boolean {
  return spec.name.startsWith(query) || spec.aliases.some((alias) => alias.startsWith(query));
}

/** The commands matching `query`, by source: Opencord first, then plugins in install order. */
export function groupCommands(
  list: CommandListRecord | undefined,
  query: string,
  context: CommandContext,
): CommandGroup[] {
  const lowered = query.toLowerCase();
  const bySource = new Map<string, CommandRow[]>([['', []]]);
  const sources = new Map<string, CommandSource>([['', OPENCORD]]);
  for (const spec of list?.commands ?? []) {
    if (!matches(spec, lowered)) continue;
    const source = sources.get(spec.source) ?? sourceOf(spec.source, context);
    sources.set(spec.source, source);
    const rows = bySource.get(spec.source) ?? [];
    bySource.set(spec.source, rows);
    rows.push({
      kind: 'command',
      key: `${spec.source}/${spec.name}`,
      spec,
      source,
      unavailable: commandUnavailable(spec, context),
    });
  }
  return [...bySource.entries()]
    .filter(([, rows]) => rows.length > 0)
    .map(([id, rows]) => ({ source: sources.get(id) ?? OPENCORD, rows }));
}

export type CommandListItem =
  | { type: 'header'; key: string; group: number; title: string }
  | { type: 'row'; key: string; group: number; row: CommandRow };

/** The list's items, each group's header first. */
export function flatRows(groups: readonly CommandGroup[]): CommandListItem[] {
  return groups.flatMap((group, index) => [
    { type: 'header' as const, key: `header/${group.source.id}/${group.source.name}`, group: index, title: group.source.name },
    ...group.rows.map((row) => ({ type: 'row' as const, key: row.key, group: index, row })),
  ]);
}

/** Where group `group`'s header is in the items. */
export function headerIndex(items: readonly CommandListItem[], group: number): number {
  return items.findIndex((item) => item.type === 'header' && item.group === group);
}
