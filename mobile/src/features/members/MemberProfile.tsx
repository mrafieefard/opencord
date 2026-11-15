import * as Clipboard from 'expo-clipboard';
import { StyleSheet, View } from 'react-native';

import { longDate } from '@/features/servers/formatTime';
import { useImageFile } from '@/features/servers/useImageFile';
import { toast } from '@/state/actions/ui';
import { useMemberColor, usePresence, useRoles, useServer } from '@/state/selectors';
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
