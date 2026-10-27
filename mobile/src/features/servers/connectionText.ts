// The connection state text for server rows (spec §8.2).
import { ConnectionRecord, ConnectionRecord_Tags } from '@opencord/core-native';

import { failureSentence } from '@/core/errors';

export function connectionText(
  connection: ConnectionRecord,
): { text: string; error: boolean } | null {
  switch (connection.tag) {
    case ConnectionRecord_Tags.Connected:
      return null;
    case ConnectionRecord_Tags.Connecting:
      return { text: 'Connecting…', error: false };
    case ConnectionRecord_Tags.Reconnecting:
      return { text: 'Reconnecting…', error: false };
    case ConnectionRecord_Tags.Failed:
      return { text: failureSentence(connection.inner.reason), error: true };
  }
}
