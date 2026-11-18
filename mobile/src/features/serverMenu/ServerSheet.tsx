// The server sheet (design §6.1): the server's icon, name and counts; Mark
// as read; the admin rows this user may open (serverRows); Leave server.
// It opens from iOS's tab slot, Android's header button, the server's name
// and the servers list's long-press menu, all at `server/<key>/settings`.
import { PresenceStatus, type ServerSnapshot } from '@opencord/core-native';
import { router } from 'expo-router';
import { useEffect, useMemo, useRef } from 'react';
import { StyleSheet, View } from 'react-native';

import { useServerRowActions } from '@/features/servers/serverRowParts';
import { useImageFile } from '@/features/servers/useImageFile';
import { useServerRows } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import type { ServerRow } from '@/state/types';
import { ServerIcon, SettingsList, SettingsRow, SettingsSection, SheetHeader, space } from '@/ui';

import { canInvite, useServerSheetRows } from './serverRows';

/** Medium, as the design asks; a page pushed in the sheet grows it to large. */
export const SHEET_DETENTS = [0.5, 1];

const ICON_SIZE = 40;

/** `N members · M online`: online is everyone whose presence isn't Offline. */
export function memberCounts(snapshot: ServerSnapshot): string {
  const status = new Map(snapshot.presences.map((presence) => [presence.userId, presence.status]));
  const total = snapshot.members.length;
  const online = snapshot.members.filter(
    (member) => (status.get(member.user.id) ?? PresenceStatus.Offline) !== PresenceStatus.Offline,
  ).length;
  return `${total} ${total === 1 ? 'member' : 'members'} · ${online} online`;
}

export function ServerSheet({ serverKey }: { serverKey: string }) {
  const rows = useServerRows('');
  const row = rows.find((candidate) => candidate.key === serverKey);
  // Left (from this sheet's own Leave, or elsewhere): nothing to show.
  const closed = useRef(false);
  useEffect(() => {
    if (row || closed.current) return;
    closed.current = true;
    router.back();
  }, [row]);
  return row ? <ServerSheetContent row={row} /> : null;
}

function ServerSheetContent({ row }: { row: ServerRow }) {
  const snapshot = useAppStore((state) => state.servers[row.key]?.snapshot);
  const counts = useMemo(() => (snapshot ? memberCounts(snapshot) : undefined), [snapshot]);
  const pages = useServerSheetRows(row.key) ?? [];
  const { markRead, confirmLeave } = useServerRowActions(row);
  const iconFile = useImageFile(row.key, row.iconHash);
  // Kept as its own view: flattened, react-native-screens places the list at
  // the top of the sheet, over the header (iOS 26; as SoundboardSheet).
  return (
    <View testID="server-sheet" collapsable={false} style={styles.sheet}>
      <View style={styles.header}>
        <SheetHeader
          title={row.name}
          subtitle={counts}
          leading={<ServerIcon name={row.name} file={iconFile} size={ICON_SIZE} />}
          onClose={() => router.back()}
        />
      </View>
      <SettingsList>
        {/* P2 adds Invite people above this, P8 Mute server under it. */}
        <SettingsSection>
          {canInvite(snapshot) && (
            <SettingsRow
              kind="link"
              label="Invite people"
              onPress={() => router.push(`/server/${row.key}/invite`)}
            />
          )}
          <SettingsRow kind="action" label="Mark as read" onPress={markRead} />
        </SettingsSection>
        {pages.length > 0 && (
          <SettingsSection>
            {pages.map(({ key, page }) => {
              if (page.route === null) return <SettingsRow key={key} kind="value" label={page.label} value={page.value} />;
              const route = page.route;
              return <SettingsRow key={key} kind="link" label={page.label} onPress={() => router.push(route(row.key))} />;
            })}
          </SettingsSection>
        )}
        <SettingsSection>
          <SettingsRow kind="destructive" label="Leave server" onPress={confirmLeave} />
        </SettingsSection>
      </SettingsList>
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: { flex: 1 },
  header: { paddingHorizontal: space.lg, paddingBottom: space.sm },
});
