import { fireEvent, render, screen } from '@testing-library/react-native';
import { router } from 'expo-router';

import {
  ConnectionRecord,
  PresenceStatus,
  type MemberRecord,
  type RoleRecord,
  type ServerAccess,
} from '@opencord/core-native';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { settle } from '@/test/store';
import { useAppStore } from '@/state/store';
import * as groupMembersModule from '../groupMembers';
import { MemberProfile } from '../MemberProfile';
import { MembersSheet } from '../MembersSheet';

const defaultAccess: ServerAccess = {
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

const TEST_ROLES: RoleRecord[] = [
  {
    id: '2001',
    name: 'Moderators',
    color: 0x5865f2,
    position: 2,
    hoist: true,
    mentionable: true,
    maxUploadBytes: undefined,
    pluginId: undefined,
    access: defaultAccess,
  },
];

const TEST_MEMBERS: MemberRecord[] = [
  {
    user: { id: '1001', displayName: 'Alice', fingerprint: 'fp-alice' },
    roleIds: ['2001'],
    joinedAtMs: 1_791_374_400_000,
  },
  {
    user: { id: '1002', displayName: 'Bob', fingerprint: 'fp-bob' },
    roleIds: [],
    joinedAtMs: 1_791_374_400_000,
  },
  {
    user: { id: '1003', displayName: 'Charlie', fingerprint: 'fp-charlie' },
    roleIds: [],
    joinedAtMs: 1_791_374_400_000,
  },
];

describe('MembersSheet', () => {
  const serverKey = NIGHT_OWLS;

  beforeEach(() => {
    jest.clearAllMocks();
    const snap = snapshot(serverKey);
    snap.ownerId = '1001';
    snap.members = TEST_MEMBERS;
    snap.roles = TEST_ROLES;
    snap.presences = [
      { userId: '1001', status: PresenceStatus.Online },
      { userId: '1002', status: PresenceStatus.Online },
      { userId: '1003', status: PresenceStatus.Offline },
    ];

    useAppStore.setState({
      myVoice: null,
      servers: {
        [serverKey]: {
          key: serverKey,
          entry: {
            key: serverKey,
            host: 'test.opencord.org',
            port: 443,
            name: 'Test Server',
            userId: '1001',
          },
          connection: new ConnectionRecord.Connected(),
          snapshot: snap,
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
    });
  });

  afterEach(async () => {
    await settle();
  });

  // Fails if member sections are not rendered with correct headers and names.
  it('the members sheet lists members grouped by role', async () => {
    await render(<MembersSheet serverKey={serverKey} />);

    // Section headers
    expect(screen.getByText('MODERATORS — 1')).toBeTruthy();
    expect(screen.getByText('ONLINE — 1')).toBeTruthy();
    expect(screen.getByText('OFFLINE — 1')).toBeTruthy();

    // Member names
    expect(screen.getByText('Alice')).toBeTruthy();
    expect(screen.getByText('Bob')).toBeTruthy();
    expect(screen.getByText('Charlie')).toBeTruthy();
  });

  // Fails if tapping a member row does not push the member profile route.
  it('tapping a row opens the profile', async () => {
    await render(<MembersSheet serverKey={serverKey} />);

    const aliceRow = screen.getByText('Alice');
    await fireEvent.press(aliceRow);

    expect(router.push).toHaveBeenCalledTimes(1);
    expect(router.push).toHaveBeenCalledWith(`/server/${serverKey}/member/1001`);
  });

  // Fails if Member since is not rendered with the formatted join date in MemberProfile.
  it('member profile renders Member since with long date format', async () => {
    await render(<MemberProfile serverKey={serverKey} userId="1001" />);

    expect(screen.getByText('MEMBER SINCE')).toBeTruthy();
    expect(screen.getByText('7 October 2026')).toBeTruthy();
  });

  // Fails if the sheet header is not padded with horizontal gutter.
  it('sheet header has horizontal padding matching content gutter', async () => {
    await render(<MembersSheet serverKey={serverKey} />);

    const headerWrap = screen.getByTestId('members-sheet-header-wrap');
    expect(headerWrap.props.style).toEqual(
      expect.objectContaining({ paddingHorizontal: 16 }),
    );
  });

  // Fails if groupMembers is called more than once when rendering 2000 members.
  it('grouping 2000 members is one pass', async () => {
    const bigMembers: MemberRecord[] = [];
    const bigPresences = [];
    for (let i = 0; i < 2000; i++) {
      const id = String(1000 + i);
      bigMembers.push({
        user: { id, displayName: `User ${i}`, fingerprint: `fp-${i}` },
        roleIds: i % 10 === 0 ? ['2001'] : [],
        joinedAtMs: 1_791_374_400_000,
      });
      bigPresences.push({
        userId: id,
        status: i % 2 === 0 ? PresenceStatus.Online : PresenceStatus.Offline,
      });
    }

    const snap = snapshot(serverKey);
    snap.ownerId = '1001';
    snap.members = bigMembers;
    snap.roles = TEST_ROLES;
    snap.presences = bigPresences;

    const currentServer = useAppStore.getState().servers[serverKey];
    if (!currentServer) throw new Error('missing server');

    useAppStore.setState({
      servers: {
        [serverKey]: {
          ...currentServer,
          snapshot: snap,
        },
      },
    });

    const spy = jest.spyOn(groupMembersModule, 'groupMembers');

    await render(<MembersSheet serverKey={serverKey} />);

    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  // Fails if members in the current user's voice channel are not grouped under 'IN THIS VOICE CHANNEL'.
  it('renders in this voice channel section when connected to voice', async () => {
    const channelId = CHANNELS.lounge;
    const snap = snapshot(serverKey);
    snap.ownerId = '1001';
    snap.members = TEST_MEMBERS;
    snap.roles = TEST_ROLES;
    snap.presences = [
      { userId: '1001', status: PresenceStatus.Online },
      { userId: '1002', status: PresenceStatus.Online },
      { userId: '1003', status: PresenceStatus.Offline },
    ];

    const currentServer = useAppStore.getState().servers[serverKey];
    if (!currentServer) throw new Error('missing server');

    useAppStore.setState({
      myVoice: {
        serverKey,
        channelId,
        link: null,
        muted: false,
        deafened: false,
      },
      servers: {
        [serverKey]: {
          ...currentServer,
          snapshot: snap,
          voice: {
            '1002': {
              userId: '1002',
              channelId,
              thisDevice: false,
              selfMute: false,
              selfDeaf: false,
              serverMute: false,
              serverDeaf: false,
            },
          },
        },
      },
    });

    const { findByText } = await render(<MembersSheet serverKey={serverKey} />);

    expect(await findByText('IN THIS VOICE CHANNEL — 1')).toBeTruthy();
  });
});
