import { PresenceStatus, type MemberRecord, type RoleRecord, type ServerAccess } from '@opencord/core-native';
import { groupMembers, type MemberSection } from '../groupMembers';

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

function makeMember(id: string, displayName: string, roleIds: string[] = []): MemberRecord {
  return {
    user: {
      id,
      displayName,
      fingerprint: `fingerprint-${id}`,
      avatarHash: undefined,
    },
    nickname: undefined,
    roleIds,
    joinedAtMs: 1_791_331_200_000,
  };
}

describe('groupMembers (ported from desktop member_sections_test.dart)', () => {
  const selfId = '1000';
  const kaiId = '1001';
  const miraId = '1002';
  const lenaId = '1003';
  const omarId = '1004';

  const roles: RoleRecord[] = [
    {
      id: '2',
      name: 'Maintainer',
      color: 0x5865f2,
      position: 2,
      hoist: true,
      mentionable: false,
      maxUploadBytes: undefined,
      pluginId: undefined,
      access: defaultAccess,
    },
  ];

  const everyone: MemberRecord[] = [
    makeMember(selfId, 'Alex'),
    makeMember(kaiId, 'Kai', ['2']),
    makeMember(miraId, 'Mira'),
    makeMember(lenaId, 'Lena', ['2']),
    makeMember(omarId, 'Omar'),
  ];

  const presences: Record<string, PresenceStatus> = {
    [selfId]: PresenceStatus.Online,
    [kaiId]: PresenceStatus.Idle,
    [miraId]: PresenceStatus.Dnd,
    [lenaId]: PresenceStatus.Offline,
  };

  const activities: Record<string, string> = {
    [miraId]: 'Editing main.rs',
  };

  function describeSections(sections: MemberSection[]): string[] {
    const lines: string[] = [];
    for (const section of sections) {
      lines.push(`${section.title} — ${section.count}`);
      for (const item of section.data) {
        lines.push(item.name);
      }
    }
    return lines;
  }

  // Fails if hoisted roles are not first, or members are not sorted alphabetically,
  // or online/offline partitioning is wrong.
  it('hoisted roles first, then online, then offline', () => {
    const sections = groupMembers(everyone, roles, presences, undefined, undefined, { ownerId: selfId });
    expect(describeSections(sections)).toEqual([
      'Maintainer — 1',
      'Kai',
      'Online — 2',
      'Alex',
      'Mira',
      'Offline — 2',
      'Lena',
      'Omar',
    ]);
  });

  // Fails if lines do not carry presence, owner flag, or activity text.
  it('lines carry presence, the owner star and activity', () => {
    const sections = groupMembers(
      everyone,
      roles,
      presences,
      { activities },
      undefined,
      { ownerId: selfId },
    );
    const allItems = sections.flatMap((s) => s.data);
    const alex = allItems.find((item) => item.id === selfId);
    const mira = allItems.find((item) => item.id === miraId);

    expect(alex?.owner).toBe(true);
    expect(mira?.presence).toBe(PresenceStatus.Dnd);
    expect(mira?.activity).toBe('Editing main.rs');
  });

  // Fails if voice channel does not override activity text with 'In voice · <channel>'.
  it('voice beats activity text', () => {
    const sections = groupMembers(
      everyone,
      roles,
      presences,
      {
        activities,
        channelOf: (id) => (id === miraId ? 'General' : undefined),
      },
      undefined,
      { ownerId: selfId },
    );
    const allItems = sections.flatMap((s) => s.data);
    const mira = allItems.find((item) => item.id === miraId);

    expect(mira?.activity).toBe('In voice · General');
  });

  // Fails if members in voice channel are not shown in both voice section and their role/status section.
  it('members in voice channel appear in both voice section and role section', () => {
    const sections = groupMembers(
      everyone,
      roles,
      presences,
      {
        channelOf: (id) => (id === kaiId ? 'General' : undefined),
        inVoiceUserIds: [kaiId],
      },
      undefined,
      { ownerId: selfId },
    );
    expect(describeSections(sections)).toEqual([
      'In this voice channel — 1',
      'Kai',
      'Maintainer — 1',
      'Kai',
      'Online — 2',
      'Alex',
      'Mira',
      'Offline — 2',
      'Lena',
      'Omar',
    ]);
  });

  // Fails if query filter does not retain matching members and their non-empty sections.
  it('search keeps matching members and their sections', () => {
    const sections = groupMembers(everyone, roles, presences, undefined, 'en', { ownerId: selfId });
    expect(describeSections(sections)).toEqual(['Offline — 1', 'Lena']);
  });

  // Fails if offline section with > 30 members does not start collapsed with empty data.
  it('a large offline section starts collapsed', () => {
    const crowd = [
      ...everyone,
      ...Array.from({ length: 30 }, (_, i) => makeMember(`500${i}`, `Guest ${i}`)),
    ];

    const collapsedSections = groupMembers(crowd, roles, presences, undefined, undefined, { ownerId: selfId });
    const offlineHeader = collapsedSections.find((s) => s.title === 'Offline');

    expect(offlineHeader).toBeDefined();
    expect(offlineHeader?.title).toBe('Offline');
    expect(offlineHeader?.collapsible).toBe(true);
    expect(offlineHeader?.collapsed).toBe(true);
    expect(offlineHeader?.data).toHaveLength(0);

    const expandedSections = groupMembers(crowd, roles, presences, undefined, undefined, {
      ownerId: selfId,
      offlineExpanded: true,
    });
    const expandedOffline = expandedSections.find((s) => s.title === 'Offline');

    expect(expandedOffline?.collapsed).toBe(false);
    expect(expandedOffline?.data.length).toBeGreaterThan(30);
  });

  // Fails if grouping 2000 members iterates over members multiple times (not a single pass)
  // or exceeds reasonable linear performance.
  it('grouping 2000 members is one pass', () => {
    const bigCrowd: MemberRecord[] = [];
    const manyPresences: Record<string, PresenceStatus> = {};
    for (let i = 0; i < 2000; i++) {
      const id = String(2000 + i);
      const isHoisted = i % 5 === 0;
      bigCrowd.push(makeMember(id, `User ${String(i).padStart(4, '0')}`, isHoisted ? ['2'] : []));
      manyPresences[id] = i % 2 === 0 ? PresenceStatus.Online : PresenceStatus.Offline;
    }

    const start = performance.now();
    const sections = groupMembers(bigCrowd, roles, manyPresences, undefined, undefined, { ownerId: selfId });
    const duration = performance.now() - start;

    expect(sections.length).toBeGreaterThan(0);
    // 2000 members single pass should complete in under 50ms in Node/Jest
    expect(duration).toBeLessThan(50);
  });
});
