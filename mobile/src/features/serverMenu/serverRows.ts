// The server sheet's admin rows (design §6.1). A row shows only when the
// desktop's permission for its page holds (serverSettingsRows) and a page
// for it exists on the phone: each plan registers its own pages, so the
// sheet never offers one that isn't built yet.
import type { ServerSnapshot } from '@opencord/core-native';
import { router, type Href } from 'expo-router';
import { useEffect, useMemo, useRef } from 'react';

import { toast } from '@/state/actions/ui';
import { serverSettingsRows, type ServerRowKey } from '@/state/selectors';
import { useAppStore } from '@/state/store';

/**
 * A page a row opens: `route` gives its place in the sheet's own stack for
 * a server, `/server/<key>/settings/<page>` (a typed route, so a page that
 * doesn't exist doesn't compile). Give it as a path string: the lost-access
 * rule (useOpenPageGuard) matches the open page by it. A row with no route
 * can't be pressed and shows `value` instead.
 */
export type ServerPage = { label: string; route: (serverKey: string) => Href } | { label: string; route: null; value: string };

export interface ServerSheetRow {
  key: ServerRowKey;
  page: ServerPage;
}

export const LOST_ACCESS = 'You no longer have access to this.';

const pages = new Map<ServerRowKey, ServerPage>();

/** Called once by the module that builds a page; a later call replaces it. */
export function registerServerPage(key: ServerRowKey, page: ServerPage): void {
  pages.set(key, page);
}

// The hosting pages stay on the desktop for now (design §3, the owner):
// the row says where to go, and opens nothing.
registerServerPage('hosting', { label: 'Hosting', route: null, value: 'Open on a computer' });

/** Whether this snapshot's user may invite people to the server (§8.6). */
export function canInvite(snapshot: ServerSnapshot | null | undefined): boolean {
  if (!snapshot) return false;
  return Boolean(snapshot.serverAccess.createInvite || snapshot.serverAccess.manageServer);
}

/** The rows this snapshot's user may open, in the desktop's order; null without a snapshot. */
export function serverSheetRows(snapshot: ServerSnapshot | null | undefined): ServerSheetRow[] | null {
  if (!snapshot) return null;
  const isOwner = snapshot.ownerId === snapshot.selfUser.id;
  return serverSettingsRows(snapshot.serverAccess, snapshot.voiceEnabled, isOwner).flatMap((key) => {
    const page = pages.get(key);
    return page ? [{ key, page }] : [];
  });
}

export function useServerSheetRows(serverKey: string): ServerSheetRow[] | null {
  const snapshot = useAppStore((state) => state.servers[serverKey]?.snapshot);
  return useMemo(() => serverSheetRows(snapshot), [snapshot]);
}

/** Opens the server sheet, from wherever the user is. */
export function openServerSheet(serverKey: string): void {
  router.push(`/server/${serverKey}/settings`);
}

/** The registered page `pathname` is on (or under, a page it pushed), if any. */
export function openPageKey(serverKey: string, pathname: string): ServerRowKey | null {
  for (const [key, page] of pages) {
    if (page.route === null) continue;
    const href = page.route(serverKey);
    if (typeof href !== 'string') continue;
    if (pathname === href || pathname.startsWith(`${href}/`)) return key;
  }
  return null;
}

/**
 * The sheet's stack calls this with where it is, so every page a row
 * opens, and every page under it, follows the rule without doing anything
 * itself: when the user loses the permission for the open page (an
 * AccessChanged, a role taken away or deleted), it closes back to the
 * sheet's list and says why, once. A request the page had in flight still
 * reports its own failure: its toast doesn't depend on the page. Without a
 * snapshot (not seeded yet) nothing is decided; a page the user closes
 * themselves says nothing.
 */
export function useOpenPageGuard(serverKey: string, pathname: string): void {
  const rows = useServerSheetRows(serverKey);
  const open = openPageKey(serverKey, pathname);
  const lost = open !== null && rows !== null && !rows.some((row) => row.key === open);
  // The page being closed, so a second change while it closes says nothing more.
  const closing = useRef<string | null>(null);
  useEffect(() => {
    if (!lost) {
      if (open === null) closing.current = null;
      return;
    }
    if (closing.current === pathname) return;
    closing.current = pathname;
    toast(LOST_ACCESS);
    router.dismissTo(`/server/${serverKey}/settings`);
  }, [lost, open, pathname, serverKey]);
}
