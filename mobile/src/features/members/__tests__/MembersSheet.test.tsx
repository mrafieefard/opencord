import { fireEvent, render, screen } from '@testing-library/react-native';
import { router } from 'expo-router';

import {
  ConnectionRecord,
  PresenceStatus,
  type MemberRecord,
  type RoleRecord,
  type ServerAccess,
} from '@opencord/core-native';
import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { useAppStore } from '@/state/store';
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
    id: 'role-mod',
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
    user: { id: 'user-1', displayName: 'Alice', fingerprint: 'fp-alice' },
    roleIds: ['role-mod'],
  },
  {
    user: { id: 'user-2', displayName: 'Bob', fingerprint: 'fp-bob' },
    roleIds: [],
  },
  {
    user: { id: 'user-3', displayName: 'Charlie', fingerprint: 'fp-charlie' },
    roleIds: [],
  },
];

describe('MembersSheet', () => {
  const serverKey = NIGHT_OWLS;

  beforeEach(() => {
    jest.clearAllMocks();
    const snap = snapshot(serverKey);
    snap.ownerId = 'user-1';
    snap.members = TEST_MEMBERS;
    snap.roles = TEST_ROLES;
    snap.presences = [
      { userId: 'user-1', status: PresenceStatus.Online },
      { userId: 'user-2', status: PresenceStatus.Online },
      { userId: 'user-3', status: PresenceStatus.Offline },
    ];

    useAppStore.setState({
      servers: {
        [serverKey]: {
          key: serverKey,
          entry: {
            key: serverKey,
            host: 'test.opencord.org',
            port: 443,
            name: 'Test Server',
            userId: 'user-1',
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
    fireEvent.press(aliceRow);

    expect(router.push).toHaveBeenCalledWith(`/server/${serverKey}/member/user-1`);
  });
});
