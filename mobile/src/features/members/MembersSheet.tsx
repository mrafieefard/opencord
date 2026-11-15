import { router } from 'expo-router';
import { useMemo, useState } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { PresenceStatus } from '@/core/native';
import { useRoles, useServer } from '@/state/selectors';
import { Icon, PaneHeader, SearchField, SheetHeader, Text, sheetLook, space, useSheetBottomInset, useTheme } from '@/ui';
import { groupMembers, type MemberItem, type MemberSection } from './groupMembers';
import { MemberRow } from './MemberRow';

export interface MembersSheetProps {
  serverKey: string;
  onClose?: () => void;
  onSelectMember?: (userId: string) => void;
  variant?: 'sheet' | 'column';
}

export function MembersSheet({ serverKey, onClose, onSelectMember, variant = 'sheet' }: MembersSheetProps) {
  const { colors } = useTheme();
  const bottomInset = useSheetBottomInset();
  const server = useServer(serverKey);
  const roles = useRoles(serverKey);
  const [query, setQuery] = useState('');
  const [offlineExpanded, setOfflineExpanded] = useState(false);

  const presences = useMemo(() => {
    const map = new Map<string, PresenceStatus>();
    for (const p of server?.snapshot?.presences ?? []) {
      map.set(p.userId, p.status);
    }
    return map;
  }, [server?.snapshot?.presences]);

  const voiceCtx = useMemo(() => {
    const channelMap = new Map<string, string>();
    for (const vs of Object.values(server?.voice ?? {})) {
      const ch = server?.snapshot?.channels.find((c) => c.id === vs.channelId);
      if (ch) channelMap.set(vs.userId, ch.name);
    }
    return {
      channelOf: (userId: string) => channelMap.get(userId),
    };
  }, [server?.voice, server?.snapshot?.channels]);

  const sections: MemberSection[] = useMemo(() => {
    return groupMembers(
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

  const close = onClose ?? (() => router.back());

  const handlePressMember = (userId: string) => {
    if (onSelectMember) {
      onSelectMember(userId);
    } else {
      router.push(`/server/${serverKey}/member/${userId}`);
    }
  };

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
      <ScrollView contentContainerStyle={styles.list}>
        {sections.map((section) => (
          <View key={section.key} style={styles.section}>
            {section.collapsible ? (
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
            ) : (
              <View style={styles.headerRow}>
                <Text variant="caption" color="textMuted" style={styles.headerText}>
                  {section.header}
                </Text>
              </View>
            )}
            {section.data.map((item: MemberItem) => (
              <MemberRow
                key={item.id}
                serverKey={serverKey}
                item={item}
                onPress={() => handlePressMember(item.id)}
              />
            ))}
          </View>
        ))}
      </ScrollView>
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
