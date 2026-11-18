import type { InviteRecord } from '@opencord/core-native';
import * as Clipboard from 'expo-clipboard';
import { router } from 'expo-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Share, StyleSheet, View } from 'react-native';
import { ScrollView } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { getCore } from '@/core';
import { toast } from '@/state/actions/ui';
import { useAppStore } from '@/state/store';
import {
  Button,
  IconButton,
  SheetHeader,
  Text,
  radius,
  space,
  typography,
  useTheme,
} from '@/ui';
import { SettingsRow, SettingsSection, type ChoiceOption } from '@/ui/settings';

const EXPIRIES: ChoiceOption[] = [
  { key: '3600', label: '1 hour' },
  { key: '86400', label: '1 day' },
  { key: '604800', label: '7 days' },
  { key: 'never', label: 'Never' },
];

const LIMITS: ChoiceOption[] = [
  { key: 'none', label: 'No limit' },
  { key: '1', label: '1 use' },
  { key: '5', label: '5 uses' },
  { key: '10', label: '10 uses' },
  { key: '25', label: '25 uses' },
  { key: '100', label: '100 uses' },
];

const DEFAULT_EXPIRY_S = 604800; // 7 days

export interface InviteSheetProps {
  serverKey: string;
}

export function InviteSheet({ serverKey }: InviteSheetProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const serverName =
    useAppStore((state) => state.servers[serverKey]?.snapshot?.name) ??
    'this server';

  const [invite, setInvite] = useState<InviteRecord | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expiry, setExpiry] = useState<number | undefined>(DEFAULT_EXPIRY_S);
  const [maxUses, setMaxUses] = useState<number | undefined>(undefined);

  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const create = useCallback(
    async (exp?: number, limit?: number) => {
      setBusy(true);
      setError(null);
      try {
        const record = await getCore().inviteCreate(serverKey, limit, exp);
        if (!mountedRef.current) return;
        setInvite(record);
      } catch (err: unknown) {
        if (!mountedRef.current) return;
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (mountedRef.current) {
          setBusy(false);
        }
      }
    },
    [serverKey],
  );

  useEffect(() => {
    let cancelled = false;
    getCore()
      .inviteCreate(serverKey, undefined, DEFAULT_EXPIRY_S)
      .then((record) => {
        if (!cancelled) setInvite(record);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [serverKey]);

  const handleExpiryChange = useCallback(
    (key: string) => {
      const newExpiry = key === 'never' ? undefined : Number(key);
      setExpiry(newExpiry);
      void create(newExpiry, maxUses);
    },
    [create, maxUses],
  );

  const handleMaxUsesChange = useCallback(
    (key: string) => {
      const newMaxUses = key === 'none' ? undefined : Number(key);
      setMaxUses(newMaxUses);
      void create(expiry, newMaxUses);
    },
    [create, expiry],
  );

  const handleCopy = useCallback(
    (textToCopy?: string) => {
      const link = textToCopy ?? invite?.link;
      if (!link) return;
      void Clipboard.setStringAsync(link);
      toast('Invite link copied');
    },
    [invite],
  );

  const handleShare = useCallback(() => {
    if (!invite?.link) return;
    void Share.share({ message: invite.link });
  }, [invite]);

  const expiryKey = expiry === undefined ? 'never' : String(expiry);
  const limitKey = maxUses === undefined ? 'none' : String(maxUses);

  return (
    <View
      testID="invite-sheet"
      style={[
        styles.container,
        {
          backgroundColor: colors.elevated,
          paddingBottom: Math.max(insets.bottom, space.lg),
        },
      ]}
    >
      <SheetHeader
        title={`Invite people to ${serverName}`}
        onClose={() => router.back()}
      />

      <ScrollView
        style={styles.scroll}
        contentContainerStyle={styles.scrollContent}
        keyboardShouldPersistTaps="handled"
        nestedScrollEnabled
      >
        <Text variant="body" color="textSecondary" style={styles.intro}>
          Anyone with this link can join {serverName}.
        </Text>

        <View
          style={[
            styles.linkBox,
            { backgroundColor: colors.chat, borderColor: colors.border },
          ]}
        >
          <Text
            variant="body"
            numberOfLines={1}
            ellipsizeMode="middle"
            style={[
              typography.mono,
              { color: invite ? colors.text : colors.textMuted },
            ]}
          >
            {invite?.link ?? 'Making a link…'}
          </Text>
        </View>

        <View style={styles.actionRow}>
          <View style={styles.buttonFlex}>
            <Button
              title="Share"
              variant="primary"
              icon="share"
              disabled={!invite || busy}
              onPress={handleShare}
            />
          </View>
          <View style={styles.buttonFlex}>
            <Button
              title="Copy"
              variant="secondary"
              icon="copy"
              disabled={!invite || busy}
              onPress={() => handleCopy()}
            />
          </View>
        </View>

        {invite?.webLink && (
          <View style={styles.webRow}>
            <Text
              variant="footnote"
              color="textSecondary"
              numberOfLines={1}
              style={styles.webText}
            >
              For a browser: {invite.webLink}
            </Text>
            <IconButton
              icon="copy"
              label="Copy web link"
              onPress={() => handleCopy(invite.webLink)}
              size={36}
            />
          </View>
        )}

        {error && (
          <View style={styles.errorBox}>
            <Text variant="footnote" color="recording">
              {error}
            </Text>
          </View>
        )}

        <View style={styles.settingsSection}>
          <SettingsSection>
            <SettingsRow
              kind="choice"
              label="Expire after"
              value={expiryKey}
              options={EXPIRIES}
              onChange={handleExpiryChange}
            />
            <SettingsRow
              kind="choice"
              label="Max uses"
              value={limitKey}
              options={LIMITS}
              onChange={handleMaxUsesChange}
            />
          </SettingsSection>
        </View>

        <View style={styles.refreshRow}>
          <Button
            title="Make a new link"
            variant="secondary"
            icon="retry"
            loading={busy}
            onPress={() => void create(expiry, maxUses)}
          />
        </View>

        <Text variant="footnote" color="textMuted" style={styles.footnote}>
          {invite?.domainId
            ? 'The link goes through a domain with a trusted certificate, so whoever opens it connects to this server and not to an impostor.'
            : 'The link carries this server’s fingerprint, so whoever opens it connects to this server and not to an impostor.'}
        </Text>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    borderTopLeftRadius: radius.sheet,
    borderTopRightRadius: radius.sheet,
  },
  scroll: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: space.lg,
    paddingTop: space.sm,
    paddingBottom: space.md,
    gap: space.md,
  },
  intro: {
    marginBottom: space.xs,
  },
  linkBox: {
    height: 48,
    borderRadius: radius.row,
    borderWidth: StyleSheet.hairlineWidth,
    justifyContent: 'center',
    paddingHorizontal: space.md,
  },
  actionRow: {
    flexDirection: 'row',
    gap: space.md,
  },
  buttonFlex: {
    flex: 1,
  },
  webRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  webText: {
    flex: 1,
    marginRight: space.sm,
  },
  errorBox: {
    paddingVertical: space.xs,
  },
  settingsSection: {
    marginTop: space.xs,
  },
  refreshRow: {
    alignItems: 'flex-start',
  },
  footnote: {
    marginTop: space.xs,
    lineHeight: 18,
  },
});
