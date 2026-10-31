// A voice channel before joining it, as Discord's: a sheet with the
// channel's name, who is in it (speaking, muted, deafened), and Join Voice,
// or why it can't be joined. Joining closes it; the channel's own page
// (VoicePane), with mute, deafen and leave, opens from the channel once in.
import { router } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { report } from '@/core';
import { useImageFile } from '@/features/servers/useImageFile';
import { joinVoice } from '@/state/actions/voice';
import { useChannelInfo, useMyVoice, useServer, useVoiceMembers } from '@/state/selectors';
import type { VoiceMember } from '@/state/types';
import { Avatar, Button, Icon, SheetHeader, Text, sheetLook, space, useSheetBottomInset, useTheme } from '@/ui';

export interface VoiceChannelSheetProps {
  serverKey: string;
  channelId: string;
  onClose?: () => void;
}

const AVATAR = 36;
/**
 * The most people listed; the rest are counted. No scrolling list: a form
 * sheet holding one sizes itself to it and cuts the rest off (iOS 26).
 */
const SHOWN = 8;

export function joinReason(
  info: ReturnType<typeof useChannelInfo>,
  voiceEnabled: boolean,
  count: number,
): string | null {
  if (!voiceEnabled) return 'Voice is off on this server';
  if (info?.access.connect === false) return "You can't join this channel";
  const limit = info?.channel.userLimit ?? 0;
  if (limit > 0 && count >= limit) return 'This channel is full';
  return null;
}

export function VoiceChannelSheet({ serverKey, channelId, onClose }: VoiceChannelSheetProps) {
  const { colors } = useTheme();
  const bottomInset = useSheetBottomInset();
  const info = useChannelInfo(serverKey, channelId);
  const server = useServer(serverKey);
  const members = useVoiceMembers(serverKey, channelId);
  const myVoice = useMyVoice();
  const [joining, setJoining] = useState(false);
  const close = onClose ?? (() => router.back());

  const here = myVoice?.serverKey === serverKey && myVoice.channelId === channelId;
  const reason = here ? null : joinReason(info, server?.snapshot?.voiceEnabled ?? true, members.length);
  const limit = info?.channel.userLimit ?? 0;
  const subtitle =
    members.length === 0
      ? "No one's here yet"
      : limit > 0
        ? `${members.length} of ${limit} in voice`
        : `${members.length} in voice`;

  const join = () => {
    setJoining(true);
    report(
      joinVoice(serverKey, channelId).then((joined) => {
        setJoining(false);
        if (joined) close();
      }, (error: unknown) => {
        setJoining(false);
        throw error;
      }),
      'join voice',
    );
  };

  return (
    <View
      style={[styles.sheet, { backgroundColor: sheetLook.background(colors), paddingBottom: bottomInset + space.lg }]}
    >
      <SheetHeader
        title={info?.channel.name ?? 'Voice'}
        subtitle={subtitle}
        leading={
          <View style={[styles.badge, { backgroundColor: sheetLook.well(colors) }]}>
            <Icon name="voiceChannel" size={20} color="voice" />
          </View>
        }
        onClose={close}
      />
      {members.length > 0 && (
        <View style={[styles.list, sheetLook.group(colors)]}>
          {members.slice(0, SHOWN).map((member) => (
            <MemberRow key={member.userId} serverKey={serverKey} member={member} />
          ))}
          {members.length > SHOWN && (
            <Text variant="footnote" color="textSecondary">
              {`and ${members.length - SHOWN} more`}
            </Text>
          )}
        </View>
      )}
      <View style={styles.join}>
        {here ? (
          <Text variant="callout" color="textSecondary" style={styles.center}>
            {"You're in this channel."}
          </Text>
        ) : (
          <Button
            title={reason ?? (joining ? 'Joining…' : 'Join Voice')}
            variant="voice"
            disabled={reason !== null || joining}
            onPress={join}
          />
        )}
      </View>
    </View>
  );
}

function MemberRow({ serverKey, member }: { serverKey: string; member: VoiceMember }) {
  const avatarFile = useImageFile(serverKey, member.avatarHash);
  const state = member.deafened ? 'deafened' : member.muted ? 'muted' : member.speaking ? 'speaking' : null;
  return (
    <View style={styles.row} accessible accessibilityLabel={state ? `${member.name}, ${state}` : member.name}>
      <Avatar name={member.name} file={avatarFile} size={AVATAR} speaking={member.speaking} />
      <Text variant="body" numberOfLines={1} style={styles.name}>
        {member.name}
      </Text>
      {member.deafened ? (
        <Icon name="deafen" size={16} color="textMuted" />
      ) : member.muted ? (
        <Icon name="micOff" size={16} color="textMuted" />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: { paddingHorizontal: space.lg },
  badge: { width: 40, height: 40, borderRadius: 20, alignItems: 'center', justifyContent: 'center' },
  list: { gap: space.sm },
  row: { flexDirection: 'row', alignItems: 'center', gap: space.md, minHeight: 44 },
  name: { flex: 1 },
  join: { paddingTop: space.lg },
  center: { textAlign: 'center' },
});
