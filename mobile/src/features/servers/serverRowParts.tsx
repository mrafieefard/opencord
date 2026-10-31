// What a server row shows and offers, shared by Android's swipeable row
// (ServerRow) and iOS's native list (ServerList.ios).
import * as Clipboard from 'expo-clipboard';
import { StyleSheet, View } from 'react-native';
import { ConnectionRecord_Tags } from '@opencord/core-native';

import { report } from '@/core';
import { leaveServer, markServerRead, retryServer } from '@/state/actions/servers';
import type { ServerRow as ServerRowType } from '@/state/types';
import { Badge, Icon, type MenuAction, ServerIcon, Text, confirm, space } from '@/ui';

import { connectionText } from './connectionText';
import { formatTime } from './formatTime';
import { useImageFile } from './useImageFile';

export const ROW_HEIGHT = 72;
export const ICON_SIZE = 52;
/** Where the hairline between rows starts: after the icon (design §9.3). */
export const ROW_INSET = space.lg + ICON_SIZE + space.md;

/** What a screen reader says for the row: name, second line, counts, voice, time. */
export function rowLabel(row: ServerRowType, second: string, time: string | null): string {
  const counts = row.mentions > 0 ? `${row.mentions} mentions` : row.unread > 0 ? `${row.unread} unread` : null;
  return [row.name, second, counts, row.inVoice ? 'in voice' : null, time].filter(Boolean).join(', ');
}

function previewLine(preview: NonNullable<ServerRowType['preview']>): string {
  return `#${preview.channelName} · ${preview.authorName}: ${preview.text}`;
}

/** The row's two lines and its label. */
export function rowText(row: ServerRowType, nowMs: number, locale: string) {
  const conn = connectionText(row.connection);
  const timeText = row.lastAtMs ? formatTime(row.lastAtMs, nowMs, locale) : null;
  const second = conn?.text ?? (row.preview ? previewLine(row.preview) : 'No messages yet');
  return { conn, timeText, second, label: rowLabel(row, second, timeText) };
}

/** The menu's actions, what each does, and the two swipe actions. */
export function useServerRowActions(row: ServerRowType) {
  const connected = row.connection.tag === ConnectionRecord_Tags.Connected;
  const confirmLeave = () => {
    confirm({
      title: `Leave ${row.name}?`,
      message: 'You can come back with an invite.',
      confirmTitle: 'Leave',
      destructive: true,
      onConfirm: () => report(leaveServer(row.key), 'leave server'),
    });
  };
  const markRead = () => markServerRead(row.key);
  const menuActions: MenuAction[] = [
    { id: 'read', title: 'Mark as read', icon: 'check' },
    ...(connected ? [] : [{ id: 'retry', title: 'Retry now', icon: 'retry' } satisfies MenuAction]),
    { id: 'copy-address', title: 'Copy address', icon: 'copy' },
    { id: 'leave', title: 'Leave server', icon: 'leaveServer', destructive: true },
  ];
  const handleAction = (id: string) => {
    switch (id) {
      case 'read':
        markRead();
        break;
      case 'retry':
        report(retryServer(row.key), 'retry server');
        break;
      case 'copy-address':
        report(Clipboard.setStringAsync(row.address), 'copy address');
        break;
      case 'leave':
        confirmLeave();
        break;
    }
  };
  return { menuActions, handleAction, confirmLeave, markRead };
}

/** The icon, the name and time, the second line and the counts. */
export function ServerRowContent({ row, nowMs, locale }: { row: ServerRowType; nowMs: number; locale: string }) {
  const iconFile = useImageFile(row.key, row.iconHash);
  const { conn, timeText, second } = rowText(row, nowMs, locale);
  return (
    <View style={styles.content}>
      <ServerIcon name={row.name} file={iconFile} size={ICON_SIZE} />
      <View style={styles.details}>
        <View style={styles.topLine}>
          <Text variant="headline" numberOfLines={1} style={styles.name}>
            {row.name}
          </Text>
          {timeText ? (
            <Text variant="footnote" color="textMuted" tabular>
              {timeText}
            </Text>
          ) : null}
        </View>
        <View style={styles.bottomLine}>
          <View style={styles.previewContainer}>
            <Text variant="callout" color={conn?.error ? 'recording' : 'textMuted'} numberOfLines={1}>
              {second}
            </Text>
          </View>
          <View style={styles.rightAccessories}>
            {row.inVoice ? (
              <View testID={`voice-glyph-${row.key}`} style={styles.accessory}>
                <Icon name="voiceChannel" color="voice" size={16} />
              </View>
            ) : null}
            {row.mentions > 0 ? <Badge count={row.mentions} mention /> : row.unread > 0 ? <Badge count={row.unread} /> : null}
          </View>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: space.md },
  details: { flex: 1, justifyContent: 'center', gap: 2 },
  topLine: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'baseline' },
  name: { flex: 1, marginRight: space.sm },
  bottomLine: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  previewContainer: { flex: 1, marginRight: space.sm },
  rightAccessories: { flexDirection: 'row', alignItems: 'center', gap: space.xs },
  accessory: { justifyContent: 'center', alignItems: 'center' },
});
