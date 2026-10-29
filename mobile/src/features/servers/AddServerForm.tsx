// The Add server sheet (spec §8.3): invite link or address, the claim token
// for an owner, and the trust step in the same sheet. The sheet fits its
// content, so each step is as tall as what it shows.
import { useState } from 'react';
import { StyleSheet, TextInput, View, type TextInputProps } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { router, useLocalSearchParams } from 'expo-router';

import { Button, Icon, SheetHeader, Text, radius, space, typography, useTheme, withAlpha } from '@/ui';
import { MAX_FONT_SCALE } from '@/ui/theme/typography';

import { TrustStep } from './TrustStep';
import { useAddServer } from './useAddServer';

export interface AddServerFormProps {
  initialLink?: string;
  onSuccess?: () => void;
}

export const FIELD_HEIGHT = 52;
/** The owner panel: a lighter well than the fields. */
const PANEL_TINT = 0.5;

export function AddServerForm({ initialLink, onSuccess }: AddServerFormProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const params = useLocalSearchParams<{ link?: string }>();
  const linkParam = initialLink ?? params.link;

  const {
    linkOrAddress,
    setLinkOrAddress,
    claimToken,
    setClaimToken,
    showClaimToken,
    setShowClaimToken,
    trustPending,
    error,
    busy,
    paste,
    submit,
    confirmTrust,
    cancelTrust,
  } = useAddServer({ initialLink: linkParam, onSuccess });

  const close = () => {
    if (router.canGoBack()) router.back();
  };
  const sheet = [styles.sheet, { backgroundColor: colors.elevated, paddingBottom: insets.bottom + space.lg }];

  if (trustPending !== null) {
    return (
      <View style={sheet}>
        <TrustStep
          address={trustPending.address}
          fingerprint={trustPending.fingerprint}
          busy={busy}
          error={error}
          onTrust={() => void confirmTrust()}
          onCancel={cancelTrust}
        />
      </View>
    );
  }

  const empty = linkOrAddress.trim() === '';
  return (
    <View style={sheet}>
      <SheetHeader
        title="Add a server"
        subtitle="Paste an invite link, or type the server's address."
        onClose={close}
      />

      <View style={styles.body}>
        <View style={styles.group}>
          <Field
            value={linkOrAddress}
            onChangeText={setLinkOrAddress}
            placeholder="opencord://… or host:port"
            accessibilityLabel="Invite link or address"
            keyboardType="url"
            autoFocus={linkParam === undefined}
            invalid={error !== null}
            onSubmitEditing={() => void submit()}
            trailing={
              empty ? (
                <Pill label="Paste" accessibilityLabel="Paste from clipboard" onPress={paste} />
              ) : (
                <ClearButton onPress={() => setLinkOrAddress('')} />
              )
            }
          />
          {error !== null && (
            <View style={styles.error}>
              <Icon name="warning" size={14} color="recording" />
              <Text variant="footnote" color="recording" accessibilityRole="alert" style={styles.errorText}>
                {error}
              </Text>
            </View>
          )}
        </View>

        <View style={[styles.owner, { backgroundColor: withAlpha(colors.chat, PANEL_TINT) }]}>
          <Touchable
            onPress={() => setShowClaimToken((prev) => !prev)}
            underlayColor={colors.selected}
            activeUnderlayOpacity={1}
          >
            <View
              accessible
              accessibilityRole="button"
              accessibilityLabel="I own this server"
              accessibilityState={{ expanded: showClaimToken }}
              style={styles.ownerRow}
            >
              <Icon name="lock" size={18} color="textSecondary" />
              <View style={styles.ownerText}>
                <Text variant="callout">I own this server</Text>
                <Text variant="footnote" color="textMuted">
                  Enter the token from its first start.
                </Text>
              </View>
              <View style={showClaimToken ? styles.chevronOpen : undefined}>
                <Icon name="jumpDown" size={20} color="textMuted" />
              </View>
            </View>
          </Touchable>
          {showClaimToken && (
            <View style={styles.tokenField}>
              <Field
                value={claimToken}
                onChangeText={setClaimToken}
                placeholder="Claim token"
                accessibilityLabel="Claim token"
                autoFocus
                onSubmitEditing={() => void submit()}
              />
            </View>
          )}
        </View>

        <Button title="Add" variant="primary" loading={busy} disabled={empty} onPress={() => void submit()} />
      </View>
    </View>
  );
}

interface FieldProps extends Pick<TextInputProps, 'value' | 'onChangeText' | 'placeholder' | 'keyboardType' | 'autoFocus' | 'onSubmitEditing'> {
  accessibilityLabel: string;
  invalid?: boolean;
  trailing?: React.ReactNode;
}

/** A 52 pt field: a well in the sheet, outlined while it has focus. */
function Field({ invalid = false, trailing, ...input }: FieldProps) {
  const { colors } = useTheme();
  const [focused, setFocused] = useState(false);
  const outline = invalid ? colors.recording : focused ? colors.textMuted : colors.border;
  return (
    <View
      style={[styles.field, { backgroundColor: colors.chat, borderColor: outline }]}
    >
      <TextInput
        {...input}
        placeholderTextColor={colors.textMuted}
        autoCapitalize="none"
        autoCorrect={false}
        returnKeyType="done"
        cursorColor={colors.text}
        selectionColor={colors.textMuted}
        maxFontSizeMultiplier={MAX_FONT_SCALE}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        style={[typography.body, styles.input, { color: colors.text }]}
      />
      {trailing}
    </View>
  );
}

function Pill({ label, accessibilityLabel, onPress }: { label: string; accessibilityLabel: string; onPress: () => void }) {
  const { colors } = useTheme();
  return (
    <Touchable onPress={onPress} underlayColor={colors.text} activeUnderlayOpacity={0.12} style={[styles.pill, { backgroundColor: colors.selected }]}>
      <View accessible accessibilityRole="button" accessibilityLabel={accessibilityLabel} style={styles.pillContent}>
        <Text variant="subhead">{label}</Text>
      </View>
    </Touchable>
  );
}

function ClearButton({ onPress }: { onPress: () => void }) {
  const { colors } = useTheme();
  return (
    <Touchable onPress={onPress} hitSlop={8} style={[styles.clear, { backgroundColor: colors.selected }]}>
      <View accessible accessibilityRole="button" accessibilityLabel="Clear" style={styles.clearContent}>
        <Icon name="close" size={14} color="textSecondary" />
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  sheet: {
    paddingHorizontal: space.lg,
  },
  body: {
    paddingTop: space.xl,
    gap: space.lg,
  },
  group: {
    gap: space.sm,
  },
  field: {
    height: FIELD_HEIGHT,
    flexDirection: 'row',
    alignItems: 'center',
    borderRadius: radius.icon,
    borderWidth: 1,
    paddingLeft: space.lg,
    paddingRight: space.sm,
    gap: space.sm,
  },
  input: {
    flex: 1,
    height: '100%',
    paddingVertical: 0,
  },
  pill: {
    height: 32,
    borderRadius: radius.row,
    overflow: 'hidden',
  },
  pillContent: {
    flex: 1,
    justifyContent: 'center',
    paddingHorizontal: space.md,
  },
  clear: {
    width: 24,
    height: 24,
    borderRadius: 12,
    marginRight: space.xs,
    overflow: 'hidden',
  },
  clearContent: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  error: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: space.xs + 2,
    paddingHorizontal: space.xs,
  },
  errorText: {
    flex: 1,
  },
  owner: {
    borderRadius: radius.icon,
    overflow: 'hidden',
  },
  ownerRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
  },
  ownerText: {
    flex: 1,
    gap: 2,
  },
  chevronOpen: {
    transform: [{ rotate: '180deg' }],
  },
  tokenField: {
    paddingHorizontal: space.md,
    paddingBottom: space.md,
  },
});
