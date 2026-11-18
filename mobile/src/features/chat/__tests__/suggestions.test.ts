import {
  ChannelKind,
  type ChannelAccess,
  type ChannelRecord,
  type MemberRecord,
  type RoleRecord,
  type ServerAccess,
  type ServerSnapshot,
  type UserRecord,
} from '@opencord/core-native';
import { applySuggestion, maxSuggestions, suggest, type Suggestion } from '../suggestions';

describe('suggestions', () => {
  const defaultServerAccess: ServerAccess = {
    manageServer: false,
    manageChannels: false,
    manageRoles: false,
    kickMembers: false,
    banMembers: false,
    createInvite: false,
    manageNicknames: false,
    changeNickname: false,
    manageMessages: false,
    mentionEveryone: false,
    managePlugins: false,
    manageSoundboard: false,
    musicManage: false,
    muteMembers: false,
    deafenMembers: false,
    moveMembers: false,
    prioritySpeaker: false,
    video: false,
    screenshare: false,
    administrator: false,
  };

  const defaultAccess: ChannelAccess = {
    channelId: '100',
    view: true,
    readHistory: true,
    send: true,
    attach: true,
    connect: false,
    speak: false,
    manageMessages: false,
    musicAdd: false,
    musicControl: false,
    musicManageQueue: false,
    musicManage: false,
    useSoundboard: false,
    useExternalSounds: false,
    sendVoiceMessages: true,
    startActivities: true,
    manageChannel: false,
  };

  function role(id: string, name: string, position: number): RoleRecord {
    return {
      id,
      name,
      color: 0,
      position,
      hoist: false,
      mentionable: false,
      access: defaultServerAccess,
    };
  }

  function user(id: string, displayName: string): UserRecord {
    return {
      id,
      displayName,
      fingerprint: `fp-${id}`,
    };
  }

  const roles: RoleRecord[] = [
    role('1', 'Everyone', 0),
    role('2', 'Moderator', 10),
    role('3', 'Admin', 20),
  ];

  const members: MemberRecord[] = [
    {
      user: user('10', 'Alex'),
      roleIds: ['1', '3'],
      joinedAtMs: 1000,
    },
    {
      user: user('20', 'Kai'),
      roleIds: ['1', '2'],
      nickname: 'Kai Nakamura',
      joinedAtMs: 2000,
    },
    {
      user: user('30', 'Mira'),
      roleIds: ['1'],
      joinedAtMs: 3000,
    },
  ];

  const channels: ChannelRecord[] = [
    { id: '100', kind: ChannelKind.Text, name: 'general', position: 0, userLimit: 0, textInVoice: false },
    { id: '200', kind: ChannelKind.Text, name: 'random', position: 1, userLimit: 0, textInVoice: false },
    { id: '300', kind: ChannelKind.Voice, name: 'lounge', position: 2, userLimit: 0, textInVoice: false },
  ];

  const snapshot: ServerSnapshot = {
    serverKey: 'test-server',
    name: 'Test Server',
    description: '',
    channels,
    members,
    roles,
    selfUser: user('10', 'Alex'),
    serverAccess: defaultServerAccess,
    presences: [],
    voiceStates: [],
    voiceEnabled: true,
    soundboard: { enabled: true, allowDefaultSounds: true, allowExternalSounds: true, cooldownMs: 0 },
    soundboardSounds: [],
    voiceMessageMaxSeconds: 300,
    pluginUsers: [],
    commands: { commands: [], resultsPublic: true },
    pluginChannelTypes: [],
    pluginActivities: [],
    activitySessions: [],
    access: [
      { ...defaultAccess, channelId: '100' },
      { ...defaultAccess, channelId: '200' },
      { ...defaultAccess, channelId: '300', send: false, attach: false, connect: true, speak: true },
    ],
  };

  const labels = (list: Suggestion[]): string[] => list.map((s) => s.insert.trim());

  test('members match from the start of any word, the name start first', () => {
    // Fails if member matching doesn't prioritize name start or match words.
    const found = suggest(snapshot, '100', { trigger: '@', query: 'a', start: 0 });
    // Alex starts with A (rank 0); Kai Nakamura has no word starting with A; Mira contains a.
    expect(labels(found)).toEqual(['@Alex']);

    const foundAll = suggest(snapshot, '100', { trigger: '@', query: '', start: 0 });
    expect(labels(foundAll)).toEqual(['@Alex', '@Kai Nakamura', '@Mira']);

    // Word start matching: 'n' matches 'Nakamura' in 'Kai Nakamura'
    const foundWord = suggest(snapshot, '100', { trigger: '@', query: 'n', start: 0 });
    expect(labels(foundWord)).toEqual(['@Kai Nakamura']);
  });

  test('channels the reader can see match by name', () => {
    // Fails if channel matching doesn't match by name query.
    expect(labels(suggest(snapshot, '100', { trigger: '#', query: 'ra', start: 0 }))).toEqual(['#random']);
  });

  test('emoji come from the shortcode search', () => {
    // Fails if emoji suggestion doesn't search emoji shortcodes.
    const found = suggest(snapshot, '100', { trigger: ':', query: 'thumbsup', start: 0 });
    expect(found.length).toBeGreaterThan(0);
    expect(found[0]?.insert).toBe('👍 ');
  });

  test('at most eight come back', () => {
    // Fails if suggestions count exceeds maxSuggestions (8).
    const found = suggest(snapshot, '100', { trigger: ':', query: 'fa', start: 0 });
    expect(found.length).toBeLessThanOrEqual(maxSuggestions);
    expect(found.length).toBe(8);
  });

  test('channels outside view are left out', () => {
    // Fails if non-text channels or channels without view access are included in # suggestions.
    const hiddenChannel: ChannelRecord = {
      id: '400',
      kind: ChannelKind.Text,
      name: 'secret-rabbit',
      position: 3,
      userLimit: 0,
      textInVoice: false,
    };
    const snapWithHidden: ServerSnapshot = {
      ...snapshot,
      channels: [...channels, hiddenChannel],
      access: [
        ...snapshot.access,
        {
          ...defaultAccess,
          channelId: '400',
          view: false, // Not viewable
        },
      ],
    };
    // 'secret-rabbit' has view: false, and 'lounge' (300) is voice, so only 'random' (200) matches 'r'
    expect(labels(suggest(snapWithHidden, '100', { trigger: '#', query: 'ra', start: 0 }))).toEqual(['#random']);
  });

  test('a member suggestion includes their highest role name', () => {
    // Fails if role is omitted or highest role position not computed.
    const found = suggest(snapshot, '100', { trigger: '@', query: 'alex', start: 0 });
    expect(found[0]?.kind).toBe('member');
    if (found[0]?.kind === 'member') {
      expect(found[0].role).toBe('Admin');
    }
  });

  test('a tap replaces the token', () => {
    // Fails if applySuggestion does not replace the token from start to caret.
    const token = { trigger: '@' as const, query: 'ka', start: 6 };
    const member = members[1];
    if (!member) throw new Error('missing member');
    const suggestion: Suggestion = {
      kind: 'member',
      member,
      role: 'Moderator',
      insert: '@Kai Nakamura ',
    };
    const result = applySuggestion('hello @ka', 9, token, suggestion);
    expect(result.text).toBe('hello @Kai Nakamura ');
    expect(result.caret).toBe(20);
  });
});
