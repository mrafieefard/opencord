import { useEffect, useState } from 'react';
import * as Clipboard from 'expo-clipboard';
import { StyleSheet, View } from 'react-native';
import { router } from 'expo-router';
import { FriendStatus, type SharedServerRecord } from '@opencord/core-native';

import { getCore, report } from '@/core';
import { longDate } from '@/features/servers/formatTime';
import { useImageFile } from '@/features/servers/useImageFile';
import { answerFriendRequest, openPrivateConversation, sendFriendRequest } from '@/state/actions/private';
import { toast } from '@/state/actions/ui';
import { useMemberColor, usePresence, useRoles, useServer } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { Avatar, Button, Text, presenceLabel, radius, space, useTheme } from '@/ui';

export interface MemberProfileProps {
  serverKey: string;
  userId: string;
}

export function MemberProfile({ serverKey, userId }: MemberProfileProps) {
  const { colors } = useTheme();
  const server = useServer(serverKey);
  const memberRecord = server?.snapshot?.members.find((m) => m.user.id === userId);
  const presence = usePresence(serverKey, userId);
  const roleColorNumber = useMemberColor(serverKey, userId);
  const allRoles = useRoles(serverKey);
  const avatarFile = useImageFile(serverKey, memberRecord?.user.avatarHash);

  const peer = memberRecord?.user.publicKeyHex;
  const [sharedServers, setSharedServers] = useState<SharedServerRecord[] | null>(null);
  useEffect(() => {
    let live = true;
    if (peer) {
      report(
        getCore()
          .privateSharedServers(peer)
          .then((list) => {
            if (live) setSharedServers(list);
          }),
        'load shared servers',
      );
    }
    return () => {
      live = false;
    };
  }, [peer]);

  const contacts = useAppStore((state) => state.private.contacts);

  if (!server || !memberRecord) {
    return (
      <View style={styles.container}>
        <Text variant="body" color="textSecondary" style={styles.leftMessage}>
          This member is not on the server any more.
        </Text>
      </View>
    );
  }

  const name = memberRecord.nickname ?? memberRecord.user.displayName;
  const isOwner = server.snapshot?.ownerId === userId;
  const memberRoles = allRoles.filter((r) => memberRecord.roleIds.includes(r.id));

  let inVoiceChannelName: string | undefined;
  if (server.voice) {
    for (const vs of Object.values(server.voice)) {
      if (vs.userId === userId) {
        const ch = server.snapshot?.channels.find((c) => c.id === vs.channelId);
        if (ch) inVoiceChannelName = ch.name;
        break;
      }
    }
  }

  const nameColor = roleColorNumber != null && roleColorNumber !== 0
    ? `#${roleColorNumber.toString(16).padStart(6, '0')}`
    : colors.text;

  const copyFingerprint = () => {
    if (memberRecord.user.fingerprint) {
      Clipboard.setStringAsync(memberRecord.user.fingerprint)
        .then(() => toast('Fingerprint copied'))
        .catch((err) => {
          console.warn('Failed to copy fingerprint', err);
          toast("Couldn't copy fingerprint");
        });
    }
  };

  const statusText = presenceLabel(presence);
  const subtitle = isOwner ? `${statusText} · Server owner` : statusText;

  const selfId = server.snapshot?.selfUser.id;
  const isSelf = userId === selfId;
  const isPluginUser = server.snapshot?.pluginUsers?.some((p) => p.userId === userId) ?? (server.pluginUsers ? userId in server.pluginUsers : false);

  const contact = peer ? contacts[peer] : undefined;

  const shared = sharedServers?.find((s) => s.serverKey === serverKey);
  const serverAllowsPrivate = shared ? shared.privateMessages : true;
  const friendsOnlyOk = shared && shared.friendsOnly ? contact?.friend === FriendStatus.Friends : true;
  const canMessage = serverAllowsPrivate && friendsOnlyOk;

  const handleMessage = () => {
    report(
      openPrivateConversation(serverKey, userId).then((c) => {
        router.push(`/(tabs)/messages/${c.peer}` as never);
      }),
      'open conversation',
    );
  };

  let friendTitle = 'Add friend';
  let friendDisabled = false;
  let friendVariant: 'primary' | 'secondary' = 'secondary';
  let friendIcon: 'check' | 'add' | undefined = 'add';
  let onFriendPress: (() => void) | undefined = () => {
    report(sendFriendRequest(serverKey, userId), 'add friend');
  };

  if (contact?.friend === FriendStatus.Friends) {
    friendTitle = 'Friends';
    friendDisabled = true;
    friendIcon = 'check';
    onFriendPress = undefined;
  } else if (contact?.friend === FriendStatus.Outgoing) {
    friendTitle = 'Pending';
    friendDisabled = true;
    friendIcon = undefined;
    onFriendPress = undefined;
  } else if (contact?.friend === FriendStatus.Incoming) {
    friendTitle = 'Accept request';
    friendVariant = 'primary';
    friendIcon = 'check';
    onFriendPress = () => {
      report(answerFriendRequest(contact.peer, true), 'accept friend request');
    };
  }

  return (
    <View style={styles.container}>
      <View style={styles.header}>
        <Avatar
          name={name}
          file={avatarFile}
          size={72}
          presence={presence}
          ringColor="elevated"
        />
        <View style={styles.names}>
          <Text variant="title2" style={{ color: nameColor }}>
            {name}
          </Text>
          <Text variant="callout" color="textSecondary">
            {subtitle}
          </Text>
          {inVoiceChannelName && (
            <Text variant="footnote" color="voice" style={styles.voiceText}>
              In voice · {inVoiceChannelName}
            </Text>
          )}
        </View>
        {!isSelf && !isPluginUser && (
          <View style={styles.actionButtons}>
            <Button
              title={friendTitle}
              variant={friendVariant}
              icon={friendIcon}
              disabled={friendDisabled}
              onPress={onFriendPress ?? (() => {})}
            />
            {canMessage && (
              <Button
                title="Message"
                variant="primary"
                icon="messages"
                onPress={handleMessage}
              />
            )}
          </View>
        )}
      </View>

      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionLabel}>
          ROLES
        </Text>
        {memberRoles.length === 0 ? (
          <Text variant="body" color="textMuted">
            No roles
          </Text>
        ) : (
          <View style={styles.chips}>
            {memberRoles.map((role) => {
              const dotColor = role.color != null && role.color !== 0
                ? `#${role.color.toString(16).padStart(6, '0')}`
                : undefined;
              return (
                <View key={role.id} style={[styles.chip, { backgroundColor: colors.surface }]}>
                  {dotColor && <View style={[styles.roleDot, { backgroundColor: dotColor }]} />}
                  <Text variant="caption" color="text">
                    {role.name}
                  </Text>
                </View>
              );
            })}
          </View>
        )}
      </View>

      {memberRecord.joinedAtMs > 0 && (
        <View style={styles.section}>
          <Text variant="caption" color="textMuted" style={styles.sectionLabel}>
            MEMBER SINCE
          </Text>
          <Text variant="body" color="text">
            {longDate(memberRecord.joinedAtMs)}
          </Text>
        </View>
      )}

      {memberRecord.user.fingerprint !== '' && (
        <View style={styles.section}>
          <Text variant="caption" color="textMuted" style={styles.sectionLabel}>
            IDENTITY KEY
          </Text>
          <View style={styles.fingerprintRow}>
            <Text variant="mono" color="text" tabular style={styles.fingerprint}>
              {memberRecord.user.fingerprint.replace(/-/g, ' ')}
            </Text>
            <Button
              title="Copy fingerprint"
              variant="plain"
              icon="copy"
              onPress={copyFingerprint}
            />
          </View>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    paddingVertical: space.lg,
    gap: space.xl,
  },
  leftMessage: {
    textAlign: 'center',
    paddingVertical: space.xl,
  },
  header: {
    alignItems: 'center',
    gap: space.md,
  },
  names: {
    alignItems: 'center',
    gap: space.xs,
  },
  voiceText: {
    marginTop: space.xs,
  },
  actionButtons: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    marginTop: space.xs,
  },
  section: {
    gap: space.xs,
  },
  sectionLabel: {
    fontWeight: '600',
    letterSpacing: 0.5,
  },
  chips: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: space.xs,
  },
  chip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    borderRadius: radius.pill,
  },
  roleDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
  },
  fingerprintRow: {
    gap: space.xs,
  },
  fingerprint: {
    letterSpacing: 1,
  },
});
