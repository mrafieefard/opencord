import { ActivityEndReason, ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import { activitySessions, buildChannelSections, canStartActivities, pluginActivities } from '../selectors';
import { onEvents, useAppStore } from '../store';

async function connected() {
  freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
  ]);
  await settle();
}

function server() {
  const found = useAppStore.getState().servers[NIGHT_OWLS];
  if (!found) throw new Error('Night Owls is not in the state');
  return found;
}

function textSection() {
  const section = buildChannelSections(server(), {}).find((entry) => entry.category?.id === CHANNELS.textCategory);
  if (!section) throw new Error('no Text section');
  return section;
}

const session = (viewerIds: string[], id = '900') => ({
  id,
  channelId: CHANNELS.lounge,
  pluginId: 'paint',
  activityKey: 'paint',
  name: 'Paint together',
  icon: 'assets/icon.png',
  pluginName: 'Shared Paint',
  starterId: '1001',
  startedAtMs: 1_761_500_000_000n,
  viewerIds,
});

describe('plugins in the phone’s state', () => {
  // Fails if plugin channels are left out of the list, or sorted apart
  // from the text channels they sit between (the desktop places them by
  // position).
  it('a plugin channel sits at its position among the others', async () => {
    await connected();
    const text = textSection();
    expect(text.items.map((item) => [item.kind, item.channel.name])).toEqual([
      ['text', 'off-topic'],
      ['plugin', 'help-desk'],
      ['plugin', 'old-board'],
    ]);
    const [, helpDesk, oldBoard] = text.items;
    expect(helpDesk).toMatchObject({ kind: 'plugin', typeName: 'Tickets', icon: 'assets/ticket.png', state: 'available' });
    expect(oldBoard).toMatchObject({ kind: 'plugin', state: 'plugin_stopped' });
  });

  // Fails if a channel keeps its old type name or icon after the server
  // says the type changed, or a type that's gone isn't shown as such.
  it('a type update renames and re-icons its channels', async () => {
    await connected();
    onEvents([
      new MobileEvent.PluginChannelTypesUpdated({
        serverKey: NIGHT_OWLS,
        types: [
          {
            pluginId: 'tickets',
            typeKey: 'tickets',
            name: 'Support desk',
            description: 'Help for everyone',
            icon: 'assets/desk.png',
            pluginName: 'Tickets',
          },
        ],
      }),
    ]);
    const text = textSection();
    expect(text.items[1]).toMatchObject({ kind: 'plugin', typeName: 'Support desk', icon: 'assets/desk.png' });
  });

  // Fails if a session isn't added, its viewers don't follow an update, or
  // a delete removes more than that session.
  it('a session is created, updated and deleted', async () => {
    await connected();
    expect(activitySessions(server(), CHANNELS.lounge)).toEqual([]);
    onEvents([
      new MobileEvent.ActivitySessionCreated({ serverKey: NIGHT_OWLS, session: session(['1001']) }),
      new MobileEvent.ActivitySessionCreated({ serverKey: NIGHT_OWLS, session: { ...session([], '901'), startedAtMs: 1_761_500_100_000n } }),
    ]);
    expect(activitySessions(server(), CHANNELS.lounge).map((s) => s.id)).toEqual(['900', '901']);
    onEvents([new MobileEvent.ActivitySessionUpdated({ serverKey: NIGHT_OWLS, session: session(['1001', '1002']) })]);
    expect(activitySessions(server(), CHANNELS.lounge)[0]?.viewerIds).toEqual(['1001', '1002']);
    onEvents([
      new MobileEvent.ActivitySessionDeleted({
        serverKey: NIGHT_OWLS,
        sessionId: '900',
        channelId: CHANNELS.lounge,
        reason: ActivityEndReason.Ended,
      }),
    ]);
    expect(activitySessions(server(), CHANNELS.lounge).map((s) => s.id)).toEqual(['901']);
  });

  // Fails if the registered activities or the start permission aren't read
  // from the server's state.
  it('activities and who may start them', async () => {
    await connected();
    expect(pluginActivities(server()).map((a) => a.name)).toEqual(['Paint together']);
    expect(canStartActivities(server(), CHANNELS.lounge)).toBe(true);
    onEvents([
      new MobileEvent.AccessChanged({
        serverKey: NIGHT_OWLS,
        access: (server().snapshot?.access ?? []).map((entry) =>
          entry.channelId === CHANNELS.lounge ? { ...entry, startActivities: false } : entry,
        ),
      }),
    ]);
    expect(canStartActivities(server(), CHANNELS.lounge)).toBe(false);
  });
});
