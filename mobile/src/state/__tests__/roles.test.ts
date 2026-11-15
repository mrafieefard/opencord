import {
  MobileEvent,
  PresenceStatus,
  type RoleRecord,
  type PresenceRecord,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, emptyServerAccess, fullAccess, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import { onEvents, useAppStore } from '../store';
import { memberColor, serverSettingsRows, type ServerRowKey } from '../selectors';

describe('roles, presence and server permissions', () => {
  // Fails if role and presence events during seeding are applied immediately or lost instead of held.
  it('role and presence events wait for seeding', async () => {
    freshStore();
    const newRole: RoleRecord = {
      id: '999',
      name: 'Tester',
      color: 0x123456,
      position: 5,
      hoist: true,
      mentionable: true,
      maxUploadBytes: undefined,
      pluginId: undefined,
      access: emptyServerAccess(),
    };
    const presenceChange: PresenceRecord = {
      userId: '1001',
      status: PresenceStatus.Dnd,
    };
    onEvents([
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
      new MobileEvent.RoleUpserted({ serverKey: NIGHT_OWLS, role: newRole }),
      new MobileEvent.PresenceChanged({ serverKey: NIGHT_OWLS, presence: presenceChange }),
    ]);

    expect(useAppStore.getState().servers[NIGHT_OWLS]?.held).toHaveLength(2);
    await settle();
    expect(useAppStore.getState().servers[NIGHT_OWLS]?.held).toHaveLength(0);

    const snap = useAppStore.getState().servers[NIGHT_OWLS]?.snapshot;
    expect(snap?.roles.find((r) => r.id === '999')).toEqual(newRole);
    expect(snap?.presences.find((p) => p.userId === '1001')?.status).toBe(PresenceStatus.Dnd);
  });

  // Fails if RoleDeleted does not strip the role id from members who held it.
  it('deleting a role takes it off every member', async () => {
    freshStore();
    onEvents([new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })]);
    await settle();

    // In fake world, member '1001' (Kai) holds role '2001' (Moderators).
    let server = useAppStore.getState().servers[NIGHT_OWLS];
    const kaiBefore = server?.snapshot?.members.find((m) => m.user.id === '1001');
    expect(kaiBefore?.roleIds).toContain('2001');

    onEvents([new MobileEvent.RoleDeleted({ serverKey: NIGHT_OWLS, roleId: '2001' })]);

    server = useAppStore.getState().servers[NIGHT_OWLS];
    expect(server?.snapshot?.roles.some((r) => r.id === '2001')).toBe(false);
    const kaiAfter = server?.snapshot?.members.find((m) => m.user.id === '1001');
    expect(kaiAfter?.roleIds).not.toContain('2001');
  });

  // Fails if AccessChanged does not replace snapshot.serverAccess.
  it('access changes replace server access', async () => {
    freshStore();
    onEvents([new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })]);
    await settle();

    const newServerAccess = {
      ...emptyServerAccess(),
      manageChannels: true,
      manageRoles: true,
    };

    onEvents([
      new MobileEvent.AccessChanged({
        serverKey: NIGHT_OWLS,
        access: [fullAccess(CHANNELS.general)],
        serverAccess: newServerAccess,
      }),
    ]);

    const server = useAppStore.getState().servers[NIGHT_OWLS];
    expect(server?.snapshot?.serverAccess).toEqual(newServerAccess);
  });

  // Fails if memberColor picks a 0-colour role or a lower-position role over a higher one.
  it('member colour comes from the highest coloured role', () => {
    const snap = snapshot(NIGHT_OWLS);
    // Kai ('1001') has role '2001' (color 0x5865f2).
    expect(memberColor(snap, '1001')).toBe(0x5865f2);

    // Give Kai an uncolored higher role and a lower colored role to check position resolution.
    const customSnap = {
      ...snap,
      roles: [
        {
          id: '1',
          name: 'LowerColored',
          color: 0x111111,
          position: 1,
          hoist: false,
          mentionable: false,
          maxUploadBytes: undefined,
          pluginId: undefined,
          access: emptyServerAccess(),
        },
        {
          id: '2',
          name: 'MiddleColored',
          color: 0x222222,
          position: 2,
          hoist: false,
          mentionable: false,
          maxUploadBytes: undefined,
          pluginId: undefined,
          access: emptyServerAccess(),
        },
        {
          id: '3',
          name: 'HighestNoColor',
          color: 0,
          position: 3,
          hoist: false,
          mentionable: false,
          maxUploadBytes: undefined,
          pluginId: undefined,
          access: emptyServerAccess(),
        },
      ],
      members: [
        {
          user: { id: '555', displayName: 'MultiRole', fingerprint: 'M' },
          roleIds: ['1', '2', '3'],
        },
        {
          user: { id: '666', displayName: 'NoColor', fingerprint: 'N' },
          roleIds: ['3'],
        },
        {
          user: { id: '777', displayName: 'NoRoles', fingerprint: 'O' },
          roleIds: [],
        },
      ],
    };

    expect(memberColor(customSnap, '555')).toBe(0x222222);
    expect(memberColor(customSnap, '666')).toBeNull();
    expect(memberColor(customSnap, '777')).toBeNull();
    expect(memberColor(customSnap, 'unknown-user')).toBeNull();
  });

  // Fails if serverSettingsRows does not match the desktop permission gates for each row.
  it('serverSettingsRows matches the desktop', () => {
    const plainMember = emptyServerAccess();
    expect(serverSettingsRows(plainMember, true)).toEqual([]);

    const manageChannelsOnly = { ...emptyServerAccess(), manageChannels: true };
    expect(serverSettingsRows(manageChannelsOnly, true)).toEqual(['channels']);

    const banMembersOnly = { ...emptyServerAccess(), banMembers: true };
    expect(serverSettingsRows(banMembersOnly, true)).toEqual(['members', 'bans']);

    const ownerAccess = {
      manageServer: true,
      manageChannels: true,
      manageRoles: true,
      kickMembers: true,
      banMembers: true,
      createInvite: true,
      manageNicknames: true,
      changeNickname: true,
      manageMessages: true,
      mentionEveryone: true,
      managePlugins: true,
      manageSoundboard: true,
      musicManage: true,
      muteMembers: true,
      deafenMembers: true,
      moveMembers: true,
      prioritySpeaker: true,
      video: true,
      screenshare: true,
      administrator: true,
    };
    const everyRow: ServerRowKey[] = [
      'overview',
      'voice',
      'commands',
      'soundboard',
      'privateMessages',
      'channels',
      'roles',
      'plugins',
      'hosting',
      'uploads',
      'members',
      'invites',
      'bans',
    ];
    expect(serverSettingsRows(ownerAccess, true, true)).toEqual(everyRow);

    const voiceOffOwner = serverSettingsRows(ownerAccess, false, true);
    expect(voiceOffOwner).not.toContain('voice');
    expect(voiceOffOwner).not.toContain('soundboard');
    // hosting is still visible because isOwner is true
    expect(voiceOffOwner).toContain('hosting');
  });
});
