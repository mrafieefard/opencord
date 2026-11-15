// Showing an activity: it opens on this device (activityHolder) and its own
// page comes up over the call (ActivityPage). From the call's tiles and
// control bar, the activities picker and a tile's menu alike.
import { router } from 'expo-router';

import { SHEETS_ABOVE_CALL } from '@/features/voice/callInset';
import { closeCall } from '@/features/voice/callPosition';
import { openActivity } from '@/state/actions/plugins';

/** The activity page's route. */
export function activityHref(serverKey: string, sessionId: string): `/server/${string}/activity/${string}` {
  return `/server/${serverKey}/activity/${sessionId}`;
}

/**
 * Opens the activity and shows its page. `replace` takes the place of the
 * screen it's called from (the picker's sheet), so there's one transition.
 */
export function showActivity(serverKey: string, channelId: string, sessionId: string, how: 'push' | 'replace' = 'push'): void {
  openActivity(serverKey, channelId, sessionId);
  // Android draws the call over its screens: put away, the page shows.
  if (!SHEETS_ABOVE_CALL) closeCall();
  const href = activityHref(serverKey, sessionId);
  if (how === 'replace') router.replace(href);
  else router.push(href);
}
