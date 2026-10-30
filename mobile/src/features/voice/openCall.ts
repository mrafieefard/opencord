// The call: a channel's voice page as a full-height sheet over everything,
// as Apple Music's player opens from its mini player. Every voice bar
// opens it, by a tap or a pull upwards, and so does the channel this
// device is in.
import { router } from 'expo-router';

export function openCall(serverKey: string, channelId: string): void {
  router.push(`/server/${serverKey}/call/${channelId}`);
}
