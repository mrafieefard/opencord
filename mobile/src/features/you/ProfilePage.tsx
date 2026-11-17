// Profile page (spec §6.2): picture, display name, status, and identity.
import { useCallback, useState } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { File, Paths } from 'expo-file-system';
import { useRouter } from 'expo-router';
import * as Sharing from 'expo-sharing';
import { Touchable } from 'react-native-gesture-handler';

import { getCore, report } from '@/core';
import { loadIdentity } from '@/core/identity';
import { pickPhoto } from '@/features/media/pickPhoto';
import { setAvatar, setStatus, toPresenceStatus } from '@/state/actions/presence';
import { toast } from '@/state/actions/ui';
import { usePrefs, useSelf } from '@/state/selectors';
import type { UserStatus } from '@/state/types';
import { Avatar, SettingsList, SettingsSection, SettingsRow, Text, space, useTheme } from '@/ui';

const STATUS_OPTIONS: { label: string; detail: string; status: UserStatus }[] = [
  { label: 'Online', detail: 'Shown as online', status: 'online' },
  { label: 'Idle', detail: 'Away for now', status: 'idle' },
  { label: 'Do not disturb', detail: 'No notifications', status: 'dnd' },
];

export function ProfilePage() {
  const router = useRouter();
  const self = useSelf();
  const { status } = usePrefs();
  const { colors } = useTheme();
  const [savingAvatar, setSavingAvatar] = useState(false);

  const core = getCore();
  const ownAvatar = core.ownAvatar();
  const hasAvatar = Boolean(ownAvatar.hash || ownAvatar.file);

  const handleEditPhoto = useCallback(async () => {
    const res = await pickPhoto({ title: 'Profile picture', allowRemove: hasAvatar });
    if (res.kind === 'picked') {
      setSavingAvatar(true);
      try {
        await setAvatar(res.file.path);
      } finally {
        setSavingAvatar(false);
      }
    } else if (res.kind === 'removed') {
      setSavingAvatar(true);
      try {
        await setAvatar(undefined);
      } finally {
        setSavingAvatar(false);
      }
    }
  }, [hasAvatar]);

  const handleCopyPublicKey = useCallback(async () => {
    let publicKeyHex = self?.publicKeyHex;
    if (!publicKeyHex) {
      const identity = await loadIdentity(core);
      if (identity) {
        const info = core.identityCheck(identity.secret, identity.name);
        publicKeyHex = info.publicKeyHex;
      }
    }
    if (publicKeyHex) {
      await Clipboard.setStringAsync(publicKeyHex);
      toast('Public key copied');
    }
  }, [core, self?.publicKeyHex]);

  const handleBackup = useCallback(async () => {
    const identity = await loadIdentity(core);
    if (!identity) throw new Error('no saved identity');
    const backupText = core.identityBackupEncode(identity.secret);
    const file = new File(Paths.cache, 'opencord-identity.txt');
    await file.write(backupText);
    await Sharing.shareAsync(file.uri, {
      dialogTitle: 'Back up your identity',
    });
  }, [core]);

  return (
    <SettingsList>
      {/* Top avatar section */}
      <View style={styles.avatarSection}>
        <View style={styles.avatarWrap}>
          <Avatar
            name={self?.displayName ?? ''}
            file={ownAvatar.file}
            size={96}
            presence={toPresenceStatus(status)}
          />
          {savingAvatar && (
            <View style={[StyleSheet.absoluteFill, styles.spinnerOverlay, { backgroundColor: colors.scrim }]}>
              <ActivityIndicator color={colors.accent} size="small" />
            </View>
          )}
        </View>
        <Touchable
          onPress={() => report(handleEditPhoto(), 'edit profile picture')}
          style={styles.editButton}
          accessible
          accessibilityRole="button"
          accessibilityLabel="Edit profile picture"
        >
          <Text variant="subhead" color="accent">
            Edit
          </Text>
        </Touchable>
      </View>

      {/* Display name */}
      <SettingsSection>
        <SettingsRow
          kind="link"
          label="Display name"
          value={self?.displayName ?? ''}
          onPress={() => router.push('/you/name')}
        />
      </SettingsSection>

      {/* Status section */}
      <SettingsSection title="Status">
        {STATUS_OPTIONS.map((option) => (
          <SettingsRow
            key={option.status}
            kind="check"
            label={option.label}
            detail={option.detail}
            checked={status === option.status}
            onPress={() => report(setStatus(option.status), 'set status')}
          />
        ))}
      </SettingsSection>

      {/* Identity section */}
      <SettingsSection
        title="Your identity"
        footer="This file is your identity. Anyone who has it can be you. Keep it somewhere safe."
      >
        <SettingsRow kind="value" label="Fingerprint" value={self?.fingerprint ?? ''} />
        <SettingsRow
          kind="action"
          label="Copy public key"
          onPress={() => report(handleCopyPublicKey(), 'copy public key')}
        />
        <SettingsRow
          kind="link"
          label="Back up your identity"
          icon="share"
          onPress={() => report(handleBackup(), 'back up identity')}
        />
      </SettingsSection>
    </SettingsList>
  );
}

const styles = StyleSheet.create({
  avatarSection: {
    alignItems: 'center',
    paddingTop: space.md,
    paddingBottom: space.sm,
  },
  avatarWrap: {
    position: 'relative',
    borderRadius: 48,
    overflow: 'hidden',
  },
  spinnerOverlay: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  editButton: {
    marginTop: space.xs,
    paddingVertical: space.xs,
    paddingHorizontal: space.sm,
  },
});
