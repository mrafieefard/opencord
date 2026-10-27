import { memo, useMemo } from 'react';
import { StyleSheet, View } from 'react-native';

import { deviceLocale } from '@/features/servers/formatTime';
import { useImageFile } from '@/features/servers/useImageFile';
import { useMember } from '@/state/selectors';
import { Avatar, Icon, Text, space, useTheme } from '@/ui';

import type { MessageBubbleItem } from './items';
import { MarkdownText } from './MarkdownText';
import { AttachmentView } from './attachments/AttachmentView';

export interface MessageBubbleProps {
  item: MessageBubbleItem;
  serverKey: string;
  onSelectChannel?: (channelId: string) => void;
  onLongPress?: () => void;
  onRetryPending?: (nonce: string) => void;
}

function formatBubbleTime(createdAtMs: number, edited: boolean, locale = deviceLocale()): string {
  const timeStr = new Intl.DateTimeFormat(locale, { timeStyle: 'short' }).format(new Date(createdAtMs));
  return edited ? `${timeStr} (edited)` : timeStr;
}

export const MessageBubble = memo(function MessageBubble({
  item,
  serverKey,
  onSelectChannel,
  onLongPress,
  onRetryPending,
}: MessageBubbleProps) {
  const { colors } = useTheme();
  const { message, pending, position, own, showAuthor, showAvatar } = item;

  const member = useMember(serverKey, message.authorId);
  const avatarFile = useImageFile(serverKey, member?.avatarHash);
  const authorName = member?.name ?? 'Someone';

  const isFailed = pending?.state === 'failed';
  const isSending = pending?.state === 'sending';
  const timeText = useMemo(
    () => formatBubbleTime(message.createdAtMs, message.editedAtMs !== undefined),
    [message.createdAtMs, message.editedAtMs],
  );

  const bubbleRadius = useMemo(() => {
    const rLarge = 18;
    const rSmall = 6;
    if (own) {
      return {
        borderTopLeftRadius: rLarge,
        borderBottomLeftRadius: rLarge,
        borderTopRightRadius: position === 'middle' || position === 'last' ? rSmall : rLarge,
        borderBottomRightRadius: position === 'first' || position === 'middle' ? rSmall : rLarge,
      };
    } else {
      return {
        borderTopRightRadius: rLarge,
        borderBottomRightRadius: rLarge,
        borderTopLeftRadius: position === 'middle' || position === 'last' ? rSmall : rLarge,
        borderBottomLeftRadius: position === 'first' || position === 'middle' ? rSmall : rLarge,
      };
    }
  }, [own, position]);

  return (
    <View
      style={[
        styles.row,
        own ? styles.rowOwn : styles.rowOther,
        position === 'last' || position === 'single' ? styles.rowGroupEnd : styles.rowInGroup,
      ]}
      testID={`bubble-${message.id}`}
    >
      {!own && (
        <View style={styles.avatarSlot}>
          {showAvatar ? (
            <Avatar name={authorName} file={avatarFile} size={32} />
          ) : (
            <View style={styles.avatarPlaceholder} />
          )}
        </View>
      )}

      <View
        style={[
          styles.bubble,
          bubbleRadius,
          { backgroundColor: own ? colors.bubbleOut : colors.bubbleIn },
          isFailed && { borderColor: colors.recording, borderWidth: 1 },
        ]}
      >
        {showAuthor && (
          <Text variant="subhead" color="accent" style={styles.authorName}>
            {authorName}
          </Text>
        )}

        {message.content.length > 0 && (
          <MarkdownText
            content={message.content}
            serverKey={serverKey}
            onSelectChannel={onSelectChannel}
          />
        )}

        {message.attachments && message.attachments.length > 0 && (
          <View style={[styles.attachments, message.content.length > 0 && styles.attachmentsMargin]}>
            {message.attachments.map((attachment) => (
              <AttachmentView
                key={attachment.id}
                serverKey={serverKey}
                attachment={attachment}
              />
            ))}
          </View>
        )}

        <View style={styles.metaRow}>
          {isSending && (
            <View style={styles.pendingGlyph}>
              <Icon name="clock" size={12} color="textMuted" />
            </View>
          )}
          {isFailed ? (
            <Text
              variant="footnote"
              color="recording"
              onPress={() => pending && onRetryPending?.(pending.nonce)}
            >
              Not sent. Tap to retry
            </Text>
          ) : (
            <Text variant="footnote" color="textMuted" tabular style={styles.timeText}>
              {timeText}
            </Text>
          )}
        </View>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    paddingHorizontal: space.md,
  },
  rowInGroup: {
    marginBottom: 2,
  },
  rowGroupEnd: {
    marginBottom: space.sm,
  },
  rowOwn: {
    justifyContent: 'flex-end',
  },
  rowOther: {
    justifyContent: 'flex-start',
  },
  avatarSlot: {
    width: 32,
    marginRight: space.xs,
    justifyContent: 'flex-end',
  },
  avatarPlaceholder: {
    width: 32,
    height: 32,
  },
  bubble: {
    maxWidth: '78%',
    paddingHorizontal: space.md,
    paddingTop: space.xs,
    paddingBottom: space.xs,
  },
  authorName: {
    fontWeight: '600',
    marginBottom: 2,
  },
  attachments: {
    gap: space.xs,
  },
  attachmentsMargin: {
    marginTop: space.xs,
  },
  metaRow: {
    flexDirection: 'row',
    alignSelf: 'flex-end',
    alignItems: 'center',
    marginTop: 2,
  },
  pendingGlyph: {
    marginRight: space.xs,
  },
  timeText: {
    fontSize: 11,
    lineHeight: 14,
  },
});
