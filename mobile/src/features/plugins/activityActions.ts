// What a long press on an activity offers (plugins M4 plan, Task 15): Open
// or Close, and End activity for its starter, a member who manages the
// call's channel, or its last viewer (no one else has it open). Ending asks
// first, in the app's confirm dialog.
import type { ActivitySessionRecord } from '@opencord/core-native';

import { endActivity } from '@/state/actions/plugins';
import type { ServerState } from '@/state/types';
import { confirm, type MenuAction } from '@/ui';

import { closeOpenActivity } from './activityHolder';
import { showActivity } from './showActivity';

export function activityActions(
  server: ServerState | undefined,
  session: ActivitySessionRecord,
  isOpen: boolean,
): MenuAction[] {
  const selfId = server?.snapshot?.selfUser.id;
  const manages = server?.snapshot?.access.find((entry) => entry.channelId === session.channelId)?.manageChannel ?? false;
  // Its last viewer may end it too: no one else has it open (as the server).
  const lastViewer = session.viewerIds.length === 1 && session.viewerIds[0] === selfId;
  const actions: MenuAction[] = [isOpen ? { id: 'close', title: 'Close' } : { id: 'open', title: 'Open' }];
  if (session.starterId === selfId || manages || lastViewer) actions.push({ id: 'end', title: 'End activity', destructive: true });
  return actions;
}

export function runActivityAction(serverKey: string, session: ActivitySessionRecord, id: string): void {
  if (id === 'open') showActivity(serverKey, session.channelId, session.id);
  else if (id === 'close') closeOpenActivity();
  else if (id === 'end') {
    confirm({
      title: `End ${session.name} for everyone?`,
      message: 'It closes for everyone in the call.',
      confirmTitle: 'End activity',
      destructive: true,
      onConfirm: () => void endActivity(serverKey, session.id),
    });
  }
}
