// One message as a bubble (spec §8.5), like Telegram: others' on the left
// with the author's avatar beside the group's last bubble and their name on
// its first, the user's own on the right. The time sits inside the last
// line where it fits: the text keeps room for it (MarkdownText `trailing`)
// and the time is drawn over that room.
import { memo, useMemo } from 'react';
import { StyleSheet, View } from 'react-native';

import { deviceLocale } from '@/features/servers/formatTime';
import { useImageFile } from '@/features/servers/useImageFile';
import { discardSend, retrySend } from '@/state/actions/chat';
import { useMember } from '@/state/selectors';
import { Avatar, Icon, Menu, Text, radius, space, useTheme, type MenuAction } from '@/ui';

import type { GroupPosition, MessageBubbleItem } from './items';
import { MarkdownText } from './MarkdownText';
import { AttachmentView } from './attachments/AttachmentView';
import { MessageMenu } from './MessageMenu';

export interface MessageBubbleProps {
  item: MessageBubbleItem;
  serverKey: string;
  /** 78 % of the pane (spec §8.5), measured by the list. */
  maxWidth: number;
  onSelectChannel?: (channelId: string) => void;
}

export const AVATAR_SIZE = 32;
const AVATAR_GAP = space.sm;
const PAD_H = space.md;
const PAD_V = 6;
/** Around pictures when a bubble has no text, so they nearly fill it. */
const MEDIA_PAD = space.xs;
const CORNER = radius.bubble;
const CORNER_JOINED = 6;

export function bubbleTime(createdAtMs: number, edited: boolean, locale = deviceLocale()): string {
  const time = new Intl.DateTimeFormat(locale, { timeStyle: 'short' }).format(new Date(createdAtMs));
  return edited ? `edited ${time}` : time;
}

/** Corners: 6 pt on the author's side where a bubble joins its group's next or previous. */
export function bubbleCorners(own: boolean, position: GroupPosition) {
  const joinedAbove = position === 'middle' || position === 'last';
  const joinedBelow = position === 'first' || position === 'middle';
  const top = joinedAbove ? CORNER_JOINED : CORNER;
  const bottom = joinedBelow ? CORNER_JOINED : CORNER;
  return own
    ? { borderTopLeftRadius: CORNER, borderBottomLeftRadius: CORNER, borderTopRightRadius: top, borderBottomRightRadius: bottom }
    : { borderTopRightRadius: CORNER, borderBottomRightRadius: CORNER, borderTopLeftRadius: top, borderBottomLeftRadius: bottom };
}

const FAILED_ACTIONS: MenuAction[] = [
  { id: 'retry', title: 'Retry', icon: 'retry' },
  { id: 'delete', title: 'Delete', icon: 'delete', destructive: true },
];

export const MessageBubble = memo(function MessageBubble({ item, serverKey, maxWidth, onSelectChannel }: MessageBubbleProps) {
  const { colors } = useTheme();
  const { message, pending, position, own, showAuthor, showAvatar } = item;

  const member = useMember(serverKey, message.authorId);
  const avatarFile = useImageFile(serverKey, member?.avatarHash);
  const authorName = member?.name ?? 'Someone';

  const failed = pending?.state === 'failed';
  const sending = pending?.state === 'sending';
  const time = useMemo(
    () => bubbleTime(message.createdAtMs, message.editedAtMs !== undefined),
    [message.createdAtMs, message.editedAtMs],
  );

  const hasText = message.content.length > 0;
  const hasAttachments = message.attachments.length > 0;
  const mediaOnly = !hasText && hasAttachments;
  // The time goes over the text's last line unless something follows the text.
  const timeOverText = hasText && !hasAttachments && !failed;
  // The clock takes an em more of the reserved room.
  const reserved = sending ? ` ${time}` : time;
  const padH = mediaOnly ? MEDIA_PAD : PAD_H;

  const retry = () => {
    if (pending) retrySend(pending.nonce);
  };
  const onFailedAction = (id: string) => {
    if (!pending) return;
    if (id === 'retry') retrySend(pending.nonce);
    else if (id === 'delete') discardSend(pending.nonce);
  };

  const meta = (
    <View style={styles.meta}>
      {sending && <Icon name="clock" size={12} color="textMuted" />}
      <Text variant="footnote" color="textMuted" tabular>
        {time}
      </Text>
    </View>
  );

  const bubble = (
    <View
      style={[
        styles.bubble,
        bubbleCorners(own, position),
        {
          maxWidth,
          paddingHorizontal: padH,
          paddingVertical: mediaOnly ? MEDIA_PAD : PAD_V,
          backgroundColor: own ? colors.bubbleOut : colors.bubbleIn,
        },
        failed && { borderColor: colors.recording, borderWidth: StyleSheet.hairlineWidth * 2 },
      ]}
    >
      {showAuthor && !own && (
        <Text variant="subhead" numberOfLines={1} style={mediaOnly && styles.mediaInset}>
          {authorName}
        </Text>
      )}

      {hasText && (
        <MarkdownText
          content={message.content}
          serverKey={serverKey}
          onSelectChannel={onSelectChannel}
          // A long press opens the message's menu (Copy text copies), so
          // the text isn't selectable: selection would take the press.
          selectable={false}
          trailing={timeOverText ? reserved : undefined}
          trailingColor={own ? colors.bubbleOut : colors.bubbleIn}
        />
      )}

      {hasAttachments && (
        <View style={[styles.attachments, hasText && styles.attachmentsAfterText]}>
          {message.attachments.map((attachment) => (
            <AttachmentView
              key={attachment.id}
              serverKey={serverKey}
              attachment={attachment}
              maxWidth={maxWidth - padH * 2}
            />
          ))}
        </View>
      )}

      {failed ? (
        <Text variant="footnote" color="recording" onPress={retry} style={[styles.failed, mediaOnly && styles.mediaInset]}>
          Not sent. Tap to retry
        </Text>
      ) : timeOverText ? (
        <View style={[styles.metaOver, { right: padH, bottom: PAD_V }]} pointerEvents="none">
          {meta}
        </View>
      ) : (
        <View style={[styles.metaRow, mediaOnly && styles.mediaInset]}>{meta}</View>
      )}
    </View>
  );

  const groupEnd = position === 'last' || position === 'single';
  return (
    <View
      style={[styles.row, own ? styles.rowOwn : null, groupEnd ? styles.groupEnd : styles.inGroup]}
      testID={`bubble-${message.id}`}
    >
      {!own && (
        <View style={styles.avatarSlot}>
          {showAvatar && <Avatar name={authorName} file={avatarFile} size={AVATAR_SIZE} />}
        </View>
      )}
      {failed ? (
        <Menu actions={FAILED_ACTIONS} onAction={onFailedAction} trigger="longPress">
          {bubble}
        </Menu>
      ) : sending ? (
        bubble
      ) : (
        <MessageMenu serverKey={serverKey} channelId={message.channelId} message={message} trigger="longPress">
          {bubble}
        </MessageMenu>
      )}
    </View>
  );
});

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    paddingHorizontal: space.md,
  },
  rowOwn: {
    justifyContent: 'flex-end',
  },
  inGroup: {
    marginBottom: 2,
  },
  groupEnd: {
    marginBottom: space.sm,
  },
  avatarSlot: {
    width: AVATAR_SIZE,
    marginRight: AVATAR_GAP,
  },
  bubble: {
    overflow: 'hidden',
  },
  mediaInset: {
    paddingHorizontal: space.sm,
    paddingVertical: 2,
  },
  attachments: {
    gap: space.xs,
  },
  attachmentsAfterText: {
    marginTop: space.sm,
  },
  meta: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 3,
  },
  metaOver: {
    position: 'absolute',
  },
  metaRow: {
    alignSelf: 'flex-end',
    marginTop: 2,
  },
  failed: {
    alignSelf: 'flex-end',
    marginTop: 2,
  },
});
