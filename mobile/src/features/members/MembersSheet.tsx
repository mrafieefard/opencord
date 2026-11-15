import { FlashList } from '@shopify/flash-list';
import { router } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { PresenceStatus } from '@/core/native';
import { useMyVoice, useRoles, useServer } from '@/state/selectors';
import { Icon, PaneHeader, SearchField, SheetHeader, Text, sheetLook, space, useSheetBottomInset, useTheme } from '@/ui';
import * as membersGrouping from './groupMembers';
import { type MemberItem, type MemberSection } from './groupMembers';
import { MemberRow } from './MemberRow';

export interface MembersSheetProps {
  serverKey: string;
  onClose?: () => void;
  onSelectMember?: (userId: string) => void;
  variant?: 'sheet' | 'column';
}

type FlatMemberRow =
  | { kind: 'header'; key: string; section: MemberSection }
  | { kind: 'member'; key: string; item: MemberItem };

export function MembersSheet({ serverKey, onClose, onSelectMember, variant = 'sheet' }: MembersSheetProps) {
  const { colors } = useTheme();
  const bottomInset = useSheetBottomInset();
  const server = useServer(serverKey);
  const roles = useRoles(serverKey);
  const myVoice = useMyVoice();
  const [query, setQuery] = useState('');
  const [offlineExpanded, setOfflineExpanded] = useState(false);

  const presences = useMemo(() => {
    const map = new Map<string, PresenceStatus>();
    for (const p of server?.snapshot?.presences ?? []) {
      map.set(p.userId, p.status);
    }
    return map;
  }, [server?.snapshot?.presences]);

  const inVoiceUserIds = useMemo(() => {
    if (!myVoice || myVoice.serverKey !== serverKey) return undefined;
    const ids: string[] = [];
    for (const vs of Object.values(server?.voice ?? {})) {
      if (vs.channelId === myVoice.channelId) {
        ids.push(vs.userId);
      }
    }
    return ids.length > 0 ? ids : undefined;
  }, [myVoice, serverKey, server?.voice]);

  const voiceCtx = useMemo(() => {
    const channelMap = new Map<string, string>();
    for (const vs of Object.values(server?.voice ?? {})) {
      const ch = server?.snapshot?.channels.find((c) => c.id === vs.channelId);
      if (ch) channelMap.set(vs.userId, ch.name);
    }
    return {
      channelOf: (userId: string) => channelMap.get(userId),
      inVoiceUserIds,
    };
  }, [server?.voice, server?.snapshot?.channels, inVoiceUserIds]);

  const sections: MemberSection[] = useMemo(() => {
    return membersGrouping.groupMembers(
      server?.snapshot?.members ?? [],
      roles,
      presences,
      voiceCtx,
      query,
      {
        offlineExpanded,
        ownerId: server?.snapshot?.ownerId,
      },
    );
  }, [server?.snapshot?.members, roles, presences, voiceCtx, query, offlineExpanded, server?.snapshot?.ownerId]);

  const flatData = useMemo<FlatMemberRow[]>(() => {
    const rows: FlatMemberRow[] = [];
    for (const section of sections) {
      rows.push({
        kind: 'header',
        key: `header:${section.key}`,
        section,
      });
      for (const item of section.data) {
        rows.push({
          kind: 'member',
          key: `${section.key}:${item.id}`,
          item,
        });
      }
    }
    return rows;
  }, [sections]);

  const close = onClose ?? (() => router.back());

  const handlePressMember = useCallback(
    (userId: string) => {
      if (onSelectMember) {
        onSelectMember(userId);
      } else {
        router.push(`/server/${serverKey}/member/${userId}`);
      }
    },
    [onSelectMember, serverKey],
  );

  const renderItem = useCallback(
    ({ item }: { item: FlatMemberRow }) => {
      if (item.kind === 'header') {
        const section = item.section;
        if (section.collapsible) {
          return (
            <Touchable
              onPress={() => setOfflineExpanded((prev) => !prev)}
              accessibilityRole="button"
              accessibilityLabel={`${section.title}, ${section.count}`}
              style={styles.headerRow}
            >
              <Text variant="caption" color="textMuted" style={styles.headerText}>
                {section.header}
              </Text>
              <Icon
                name={section.collapsed ? 'arrowDown' : 'arrowUp'}
                size={14}
                color="textMuted"
              />
            </Touchable>
          );
        }
        return (
          <View style={styles.headerRow}>
            <Text variant="caption" color="textMuted" style={styles.headerText}>
              {section.header}
            </Text>
          </View>
        );
      }

      return (
        <MemberRow
          serverKey={serverKey}
          item={item.item}
          onPress={() => handlePressMember(item.item.id)}
        />
      );
    },
    [serverKey, handlePressMember],
  );

  const bg = variant === 'column' ? colors.chat : sheetLook.background(colors);
  const paddingBottom = variant === 'column' ? space.lg : bottomInset + space.lg;

  return (
    <View style={[styles.container, { backgroundColor: bg, paddingBottom }]}>
      {variant === 'column' ? (
        <PaneHeader title="Members" insetTop={false} />
      ) : (
        <SheetHeader title="Members" onClose={close} />
      )}
      <View style={styles.search}>
        <SearchField
          value={query}
          onChangeText={setQuery}
          placeholder="Search members"
          onSheet
        />
      </View>
      <FlashList
        data={flatData}
        renderItem={renderItem}
        getItemType={(item) => item.kind}
        keyExtractor={(item) => item.key}
        contentContainerStyle={styles.list}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  search: {
    paddingHorizontal: space.lg,
    paddingBottom: space.md,
  },
  list: {
    paddingBottom: space.xl,
  },
  section: {
    marginBottom: space.sm,
  },
  headerRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: space.lg,
    paddingVertical: space.xs,
  },
  headerText: {
    fontWeight: '600',
    letterSpacing: 0.5,
  },
});
