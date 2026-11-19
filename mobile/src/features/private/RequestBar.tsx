// In the composer's place while the chat is a message request (design
// §6.3, the desktop's request_bar.dart): Accept, Ignore, Block.
import { StyleSheet, View } from 'react-native';

import { report } from '@/core';
import { acceptMessageRequest, blockPeer, ignoreMessageRequest } from '@/state/actions/private';
import { Button, GlassSurface, Text, confirm, space } from '@/ui';

import { useDockBottom } from '@/features/chat/dock';

/** The desktop's confirmBlock body. */
export const BLOCK_MESSAGE =
  "They won't be able to send you messages or friend requests, and you won't see them in servers you share.";

export interface RequestBarProps {
  peer: string;
  name: string;
  /** The chat is gone (ignored) or closed to them (blocked): leave it. */
  onLeave?: () => void;
}

export function RequestBar({ peer, name, onLeave }: RequestBarProps) {
  const bottom = useDockBottom();

  const accept = () => report(acceptMessageRequest(peer), 'accept request');
  const ignore = () =>
    confirm({
      title: 'Ignore request?',
      message: 'This removes the conversation.',
      confirmTitle: 'Ignore',
      destructive: true,
      onConfirm: () => report(ignoreMessageRequest(peer).then(() => onLeave?.()), 'ignore request'),
    });
  const block = () =>
    confirm({
      title: `Block ${name}?`,
      message: BLOCK_MESSAGE,
      confirmTitle: 'Block',
      destructive: true,
      onConfirm: () => report(blockPeer(peer).then(() => onLeave?.()), 'block'),
    });

  return (
    <View style={[styles.dock, { paddingBottom: bottom }]} pointerEvents="box-none">
      <GlassSurface style={styles.card} testID="request-bar">
        <Text variant="callout">{`${name} wants to message you.`}</Text>
        <View style={styles.buttons}>
          <View style={styles.button}>
            <Button title="Accept" variant="primary" onPress={accept} />
          </View>
          <View style={styles.button}>
            <Button title="Ignore" variant="secondary" onPress={ignore} />
          </View>
          <View style={styles.button}>
            <Button title="Block" variant="secondary" onPress={block} />
          </View>
        </View>
      </GlassSurface>
    </View>
  );
}

const styles = StyleSheet.create({
  dock: { paddingHorizontal: space.sm },
  card: { borderRadius: 22, padding: space.md, gap: space.md },
  buttons: { flexDirection: 'row', gap: space.sm },
  button: { flex: 1 },
});
