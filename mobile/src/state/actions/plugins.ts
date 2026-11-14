// Activities on this device (plugins M4 Task 8): starting one in this
// device's call and ending one for everyone. A refusal shows the desktop's
// words as a toast.
import { MobileError, MobileError_Tags, type ActivitySessionRecord } from '@opencord/core-native';

import { describeError, getCore } from '@/core';

import { useAppStore } from '../store';
import { toast } from './ui';

/** The most activities one call holds (the server's limit). */
export const ACTIVITY_SESSIONS_PER_CALL = 4;

function serverCode(error: unknown): string | null {
  return MobileError.instanceOf(error) && error.tag === MobileError_Tags.Server ? error.inner.code : null;
}

/** Starts an activity in the call; the running session when it runs already. Null when refused. */
export async function startActivity(
  serverKey: string,
  channelId: string,
  pluginId: string,
  activityKey: string,
): Promise<ActivitySessionRecord | null> {
  try {
    return await getCore().activityStart(serverKey, channelId, pluginId, activityKey);
  } catch (error: unknown) {
    const code = serverCode(error);
    toast(
      code === 'plugin_unavailable'
        ? "This activity isn't available right now."
        : code === 'plugin_limit'
          ? `This call already has ${ACTIVITY_SESSIONS_PER_CALL} activities.`
          : code === 'forbidden'
            ? "You don't have permission to start activities here."
            : describeError(error),
      'error',
    );
    return null;
  }
}

/** Ends an activity for everyone in its call. */
export async function endActivity(serverKey: string, sessionId: string): Promise<void> {
  try {
    await getCore().activityEnd(serverKey, sessionId);
  } catch (error: unknown) {
    toast(describeError(error), 'error');
  }
}

/** Opens an activity on this device (Task 15 draws it); another open one closes. */
export function openActivity(serverKey: string, channelId: string, sessionId: string): void {
  useAppStore.setState({ openActivity: { serverKey, channelId, sessionId } });
}
