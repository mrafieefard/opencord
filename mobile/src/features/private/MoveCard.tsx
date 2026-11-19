// They moved the chat to another server (design §6.3, the desktop's
// move_card.dart): Move too follows them, when this device is on that
// server with them.
import type { SharedServerRecord } from '@opencord/core-native';
import { StyleSheet, View } from 'react-native';

import { report } from '@/core';
import { movePrivateToo } from '@/state/actions/private';
import { Button, Text, radius, space, useTheme } from '@/ui';

export interface MoveCardProps {
  peer: string;
  name: string;
  messageId: string;
  serverName: string;
  serverKey?: string;
  /** Where the chat goes now, on this device. */
  relayServerKey?: string;
  /** The servers this device shares with them (`private_shared_servers`). */
  sharedServers: readonly SharedServerRecord[];
}

export function MoveCard({ peer, name, messageId, serverName, serverKey, relayServerKey, sharedServers }: MoveCardProps) {
  const { colors } = useTheme();
  // Already there (this device moved too, or another of its devices did).
  const here = serverKey !== undefined && relayServerKey === serverKey;
  // Move too needs the server on this device with them in it.
  const canMoveToo = !here && serverKey !== undefined && sharedServers.some((server) => server.serverKey === serverKey);
  const text =
    canMoveToo || here
      ? `${name} moved this chat to ${serverName}`
      : `${name} moved this chat to ${serverName}, a server this device isn't on.`;

  return (
    <View style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]} testID="move-card">
      <Text variant="body" style={styles.text}>
        {text}
      </Text>
      {canMoveToo && (
        <Button title="Move too" variant="primary" onPress={() => report(movePrivateToo(peer, messageId), 'move too')} />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    marginHorizontal: space.lg,
    marginVertical: space.sm,
    padding: space.md,
    borderRadius: radius.bubble,
    borderWidth: StyleSheet.hairlineWidth,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  text: { flex: 1 },
});
