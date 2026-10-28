import { memo, useMemo } from 'react';
import { Linking, StyleSheet, Text as NativeText, View, type StyleProp, type TextStyle } from 'react-native';

import { useChannelInfo, useMemberName } from '@/state/selectors';
import { fonts, typography } from '@/ui/theme/typography';
import { space } from '@/ui/theme/space';
import { useTheme } from '@/ui/theme/useTheme';

import {
  MdChannelMention,
  MdCode,
  MdInline,
  MdInlineCode,
  MdLink,
  MdParagraph,
  MdQuote,
  MdText,
  MdUserMention,
  parseMarkdown,
  type MdBlock,
} from './markdown';

export interface MarkdownTextProps {
  content: string;
  serverKey: string;
  onSelectChannel?: (channelId: string) => void;
  style?: StyleProp<TextStyle>;
  selectable?: boolean;
  /**
   * Room kept at the end of the last line for the bubble's time, which the
   * bubble draws over it (the desktop's "trailing" trick). When the text
   * ends in a quote or code, the room gets a line of its own.
   */
  trailing?: string;
  /** The bubble's colour: the room is drawn in it, since Android ignores `transparent` on nested text. */
  trailingColor?: string;
}

/** Text the width of the bubble's time, at its size, in the bubble's colour. */
function TrailingSpace({ text, color }: { text: string; color?: string }) {
  return (
    <NativeText style={[styles.trailing, { color }]} accessible={false} importantForAccessibility="no">
      {`\u2003${text}`}
    </NativeText>
  );
}

function UserMentionSpan({ serverKey, userId }: { serverKey: string; userId: string }) {
  const { colors } = useTheme();
  const name = useMemberName(serverKey, userId);

  return (
    <NativeText
      style={[
        styles.mention,
        { backgroundColor: colors.mentionBg, color: colors.text },
      ]}
    >
      @{name}
    </NativeText>
  );
}

function ChannelMentionSpan({
  serverKey,
  channelId,
  onSelectChannel,
}: {
  serverKey: string;
  channelId: string;
  onSelectChannel?: (channelId: string) => void;
}) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  const name = info?.channel.name ? `#${info.channel.name}` : `#${channelId}`;

  return (
    <NativeText
      onPress={onSelectChannel ? () => onSelectChannel(channelId) : undefined}
      style={[
        styles.mention,
        { backgroundColor: colors.mentionBg, color: colors.text },
      ]}
    >
      {name}
    </NativeText>
  );
}

function renderInlines(
  inlines: MdInline[],
  serverKey: string,
  onSelectChannel?: (channelId: string) => void,
  colors?: ReturnType<typeof useTheme>['colors'],
) {
  return inlines.map((inline, index) => {
    if (inline instanceof MdText) {
      let fontFamily: string | undefined;
      if (inline.bold && inline.italic) {
        fontFamily = fonts.semiBoldItalic;
      } else if (inline.bold) {
        // The desktop's bold is weight 600.
        fontFamily = fonts.semiBold;
      } else if (inline.italic) {
        fontFamily = fonts.italic;
      }
      return (
        <NativeText
          key={index}
          style={[
            fontFamily ? { fontFamily } : undefined,
            inline.strike ? { textDecorationLine: 'line-through' } : undefined,
          ]}
        >
          {inline.text}
        </NativeText>
      );
    }

    if (inline instanceof MdInlineCode) {
      return (
        <NativeText
          key={index}
          style={[
            styles.code,
            { fontFamily: fonts.mono, backgroundColor: colors?.mentionBg, color: colors?.text },
          ]}
        >
          {inline.code}
        </NativeText>
      );
    }

    if (inline instanceof MdLink) {
      return (
        <NativeText
          key={index}
          onPress={() => Linking.openURL(inline.url)}
          style={[styles.link, { color: colors?.text }]}
        >
          {inline.url}
        </NativeText>
      );
    }

    if (inline instanceof MdUserMention) {
      return (
        <UserMentionSpan
          key={index}
          serverKey={serverKey}
          userId={String(inline.userId)}
        />
      );
    }

    if (inline instanceof MdChannelMention) {
      return (
        <ChannelMentionSpan
          key={index}
          serverKey={serverKey}
          channelId={String(inline.channelId)}
          onSelectChannel={onSelectChannel}
        />
      );
    }

    return null;
  });
}

export const MarkdownText = memo(function MarkdownText({
  content,
  serverKey,
  onSelectChannel,
  style,
  selectable = true,
  trailing,
  trailingColor,
}: MarkdownTextProps) {
  const { colors } = useTheme();
  const blocks = useMemo(() => parseMarkdown(content), [content]);
  const last = blocks.length - 1;
  const endsInParagraph = blocks[last] instanceof MdParagraph;

  return (
    <View>
      {blocks.map((block: MdBlock, index: number) => {
        if (block instanceof MdParagraph) {
          return (
            <NativeText
              key={index}
              selectable={selectable}
              style={[typography.body, { color: colors.text }, index > 0 && styles.paragraph, style]}
            >
              {renderInlines(block.inlines, serverKey, onSelectChannel, colors)}
              {trailing !== undefined && index === last && <TrailingSpace text={trailing} color={trailingColor} />}
            </NativeText>
          );
        }

        if (block instanceof MdQuote) {
          return (
            <View
              key={index}
              style={[styles.quoteContainer, { borderLeftColor: colors.textMuted }]}
            >
              <NativeText
                selectable={selectable}
                style={[typography.body, { color: colors.textSecondary }, style]}
              >
                {renderInlines(block.inlines, serverKey, onSelectChannel, colors)}
              </NativeText>
            </View>
          );
        }

        if (block instanceof MdCode) {
          return (
            <View
              key={index}
              style={[styles.codeBlock, { backgroundColor: colors.mentionBg }]}
            >
              <NativeText
                selectable={selectable}
                style={[typography.mono, { color: colors.text }]}
              >
                {block.code}
              </NativeText>
            </View>
          );
        }

        return null;
      })}
      {trailing !== undefined && !endsInParagraph && (
        <NativeText style={typography.body}>
          <TrailingSpace text={trailing} color={trailingColor} />
        </NativeText>
      )}
    </View>
  );
});

const styles = StyleSheet.create({
  paragraph: {
    marginTop: space.xs,
  },
  trailing: {
    ...typography.footnote,
  },
  quoteContainer: {
    borderLeftWidth: 3,
    paddingLeft: space.sm,
    marginVertical: space.xs,
  },
  codeBlock: {
    padding: space.sm,
    borderRadius: 8,
    marginVertical: space.xs,
  },
  mention: {
    fontFamily: fonts.semiBold,
    paddingHorizontal: 2,
    borderRadius: 4,
  },
  code: {
    paddingHorizontal: 4,
    borderRadius: 4,
    fontSize: 13,
  },
  link: {
    textDecorationLine: 'underline',
  },
});
