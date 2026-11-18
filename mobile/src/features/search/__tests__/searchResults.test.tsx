import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { router } from 'expo-router';
import type { ReactElement } from 'react';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ChannelKind, ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import {
  collectSwitcherItems,
  rankSwitcher,
  type SwitcherChannelItem,
  type SwitcherItem,
  type SwitcherMemberItem,
  type SwitcherServerItem,
} from '../searchResults';
import { SearchResults } from '../SearchResults';

jest.mock('expo-router', () => ({
  router: { push: jest.fn(), back: jest.fn() },
}));

const devServer = 'dev:7710';
const berlinServer = 'berlin:7710';

function channel(
  server: string,
  id: string,
  name: string,
  kind: ChannelKind = ChannelKind.Text,
): SwitcherChannelItem {
  return {
    kind: 'channel',
    serverKey: server,
    serverName: server === devServer ? 'Opencord Dev' : 'Rust Berlin',
    channelId: id,
    name,
    channelKind: kind,
  };
}

const items: SwitcherItem[] = [
  channel(devServer, '1', 'general'),
  channel(devServer, '2', 'dev-core'),
  channel(devServer, '3', 'design-review'),
  channel(berlinServer, '4', 'general'),
  channel(berlinServer, '5', 'meetup-planning'),
  channel(devServer, '6', 'General', ChannelKind.Voice),
  { kind: 'server', serverKey: devServer, name: 'Opencord Dev' } as SwitcherServerItem,
  { kind: 'server', serverKey: berlinServer, name: 'Rust Berlin' } as SwitcherServerItem,
  { kind: 'member', serverKey: devServer, userId: '1001', name: 'Kai Nakamura' } as SwitcherMemberItem,
  { kind: 'member', serverKey: devServer, userId: '1002', name: 'Mira Okafor' } as SwitcherMemberItem,
];

function names(results: SwitcherItem[]): string[] {
  return results.map((item) => item.name);
}

const draw = (element: ReactElement) =>
  render(<GestureHandlerRootView style={{ flex: 1 }}>{element}</GestureHandlerRootView>);

describe('Switcher results (Task 9)', () => {
  // Fails if prefix matches do not score higher than word start matches, which score higher than substring matches.
  it('prefix matches beat word matches, which beat contains', () => {
    expect(names(rankSwitcher(items, 'de'))).toEqual([
      'design-review',
      'dev-core',
      'Opencord Dev',
    ]);
  });

  // Fails if word start splitting on whitespace, dashes, and underscores fails.
  it('word starts match across dashes and spaces', () => {
    expect(names(rankSwitcher(items, 'plan'))).toEqual(['meetup-planning']);
    expect(names(rankSwitcher(items, 'nak'))).toEqual(['Kai Nakamura']);
  });

  // Fails if #, @, * prefixes do not filter by channel, member, and server respectively.
  it('a mark in front narrows the kind', () => {
    expect(
      new Set(rankSwitcher(items, '#gen').map((item) => item.kind)),
    ).toEqual(new Set(['channel']));
    expect(names(rankSwitcher(items, '@mi'))).toEqual(['Mira Okafor']);
    expect(names(rankSwitcher(items, '*'))).toEqual(['Opencord Dev', 'Rust Berlin']);
  });

  // Fails if channels do not rank ahead of servers and servers ahead of members on equal scores.
  it('ties go to channels before servers before members', () => {
    const ranked = rankSwitcher([
      { kind: 'server', serverKey: devServer, name: 'Opencord' },
      channel(devServer, '9', 'opencord'),
    ], 'open');
    expect(ranked[0]?.kind).toBe('channel');
  });

  // Fails if empty query does not order recent channels first.
  it('an empty query shows recent channels first', () => {
    const recent = [
      { server: berlinServer, channel: '5' },
      { server: devServer, channel: '2' },
    ];
    const ranked = rankSwitcher(items, '', recent);
    expect(names(ranked).slice(0, 2)).toEqual(['meetup-planning', 'dev-core']);
  });

  // Fails if duplicate channel names on different servers lose server distinction.
  it('channels of the same name stay apart by server', () => {
    const general = rankSwitcher(items, 'general').filter(
      (item): item is SwitcherChannelItem => item.kind === 'channel',
    );
    expect(new Set(general.map((item) => item.serverName))).toEqual(
      new Set(['Opencord Dev', 'Rust Berlin']),
    );
  });

  // Fails if tapping a channel result does not navigate to its server and channel.
  it('a channel result opens the chat', async () => {
    freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    await draw(<SearchResults query="general" />);

    const channelRow = screen.getByRole('button', { name: /#general/i });
    expect(channelRow).toBeTruthy();

    await act(async () => {
      fireEvent.press(channelRow);
    });

    expect(router.push).toHaveBeenCalledWith(`/servers/${NIGHT_OWLS}?channel=10`);
  });

  // Fails if a member with a nickname cannot be found by their nickname (Item 9).
  it('a member can be found by their nickname', () => {
    const snap = snapshot(NIGHT_OWLS);
    const memberItems = collectSwitcherItems({
      servers: {
        'test-srv': {
          key: 'test-srv',
          entry: { key: 'test-srv', name: 'Test Server', host: '127.0.0.1', port: 4433 },
          connection: new ConnectionRecord.Connected(),
          snapshot: {
            ...snap,
            serverKey: 'test-srv',
            name: 'Test Server',
            members: [
              {
                user: { id: '101', displayName: 'Robert Smith', fingerprint: 'fp101' },
                nickname: 'Bob',
                roleIds: [],
                joinedAtMs: 0,
              },
            ],
          },
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
      order: ['test-srv'],
    });

    const bob = rankSwitcher(memberItems, '@bob');
    expect(names(bob)).toEqual(['Bob']);
  });
});
