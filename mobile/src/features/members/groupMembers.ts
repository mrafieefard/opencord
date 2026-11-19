import { PresenceStatus, type MemberRecord, type RoleRecord } from '@opencord/core-native';

export const OFFLINE_COLLAPSE_AT = 30;

export interface MemberItem {
  id: string;
  name: string;
  avatarHash?: string;
  fingerprint: string;
  roleIds: string[];
  presence: PresenceStatus;
  activity?: string;
  roleColor?: number | null;
  owner?: boolean;
  inVoiceSection?: boolean;
}

export interface MemberSection {
  key: string;
  title: string;
  count: number;
  header: string;
  collapsible?: boolean;
  collapsed?: boolean;
  data: MemberItem[];
}

export interface VoiceContext {
  channelOf?: (userId: string) => string | undefined;
  inVoiceUserIds?: string[];
  activities?: Record<string, string>;
}

export interface GroupMembersOptions {
  offlineExpanded?: boolean;
  ownerId?: string;
}

export function groupMembers(
  members: MemberRecord[],
  roles: RoleRecord[],
  presences: Record<string, PresenceStatus> | Map<string, PresenceStatus> | undefined,
  voice?: VoiceContext,
  query?: string,
  options?: GroupMembersOptions,
): MemberSection[] {
  const needle = query?.trim().toLowerCase() ?? '';
  const ownerId = options?.ownerId;

  // Pre-sort hoisted roles by position descending (and map for O(1) hoisted role id checks)
  const hoistedRoles = [...roles]
    .filter((r) => r.hoist)
    .sort((a, b) => b.position - a.position);

  // Pre-sort all colored roles by position descending for fast color lookup
  const coloredRoles = [...roles]
    .filter((r) => r.color !== 0 && r.color != null)
    .sort((a, b) => b.position - a.position);

  const roleBuckets = new Map<string, MemberItem[]>();
  for (const role of hoistedRoles) {
    roleBuckets.set(role.id, []);
  }

  const voiceSet = voice?.inVoiceUserIds ? new Set(voice.inVoiceUserIds) : null;
  const voiceItems: MemberItem[] = [];
  const onlineItems: MemberItem[] = [];
  const offlineItems: MemberItem[] = [];

  const getPresence = (userId: string): PresenceStatus => {
    if (!presences) return PresenceStatus.Offline;
    if (presences instanceof Map) {
      return presences.get(userId) ?? PresenceStatus.Offline;
    }
    return presences[userId] ?? PresenceStatus.Offline;
  };

  const byName = (a: MemberItem, b: MemberItem) => a.name.toLowerCase().localeCompare(b.name.toLowerCase());

  // Single pass over all members
  for (let i = 0; i < members.length; i++) {
    const member = members[i];
    if (!member) continue;
    const displayName = member.nickname ?? member.user.displayName;
    if (needle !== '' && !displayName.toLowerCase().includes(needle)) {
      continue;
    }

    const userId = member.user.id;
    const presence = getPresence(userId);

    // Color from highest colored role
    let roleColor: number | null = null;
    for (let c = 0; c < coloredRoles.length; c++) {
      const cr = coloredRoles[c];
      if (cr && member.roleIds.includes(cr.id)) {
        roleColor = cr.color;
        break;
      }
    }

    // Voice channel & activity text
    const vc = voice?.channelOf?.(userId);
    const activity = vc ? `In voice · ${vc}` : voice?.activities?.[userId];
    const isOwner = ownerId !== undefined && ownerId === userId;

    const item: MemberItem = {
      id: userId,
      name: displayName,
      avatarHash: member.user.avatarHash,
      fingerprint: member.user.fingerprint,
      roleIds: member.roleIds,
      presence,
      activity,
      roleColor,
      owner: isOwner,
    };

    if (voiceSet && voiceSet.has(userId)) {
      voiceItems.push({ ...item, inVoiceSection: true });
    }

    if (presence === PresenceStatus.Offline) {
      offlineItems.push(item);
    } else {
      // Find highest hoisted role
      let assignedRole: RoleRecord | null = null;
      for (let r = 0; r < hoistedRoles.length; r++) {
        const hr = hoistedRoles[r];
        if (hr && member.roleIds.includes(hr.id)) {
          assignedRole = hr;
          break;
        }
      }

      if (assignedRole) {
        roleBuckets.get(assignedRole.id)?.push(item);
      } else {
        onlineItems.push(item);
      }
    }
  }

  const sections: MemberSection[] = [];

  // 1. "In this voice channel"
  if (voiceItems.length > 0) {
    voiceItems.sort(byName);
    sections.push({
      key: 'section:voice',
      title: 'In this voice channel',
      count: voiceItems.length,
      header: `IN THIS VOICE CHANNEL — ${voiceItems.length}`,
      data: voiceItems,
    });
  }

  // 2. One section per hoisted role
  for (let r = 0; r < hoistedRoles.length; r++) {
    const hr = hoistedRoles[r];
    if (!hr) continue;
    const bucket = roleBuckets.get(hr.id);
    if (bucket && bucket.length > 0) {
      bucket.sort(byName);
      sections.push({
        key: `section:${hr.id}`,
        title: hr.name,
        count: bucket.length,
        header: `${hr.name.toUpperCase()} — ${bucket.length}`,
        data: bucket,
      });
    }
  }

  // 3. Online
  if (onlineItems.length > 0) {
    onlineItems.sort(byName);
    sections.push({
      key: 'section:online',
      title: 'Online',
      count: onlineItems.length,
      header: `ONLINE — ${onlineItems.length}`,
      data: onlineItems,
    });
  }

  // 4. Offline
  if (offlineItems.length > 0) {
    offlineItems.sort(byName);
    const collapsible = offlineItems.length > OFFLINE_COLLAPSE_AT;
    const collapsed = collapsible && !options?.offlineExpanded;
    sections.push({
      key: 'section:offline',
      title: 'Offline',
      count: offlineItems.length,
      header: `OFFLINE — ${offlineItems.length}`,
      collapsible,
      collapsed,
      data: collapsed ? [] : offlineItems,
    });
  }

  return sections;
}
