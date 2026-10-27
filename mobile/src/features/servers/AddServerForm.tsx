// The Add server form (spec §8.3): invite link or address, optional claim token,
// and the in-sheet trust step.
import { StyleSheet, TextInput, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useLocalSearchParams } from 'expo-router';

import { Button, Icon, Text, radius, space, typography, useTheme } from '@/ui';
import { MAX_FONT_SCALE } from '@/ui/theme/typography';

import { TrustStep } from './TrustStep';
import { useAddServer } from './useAddServer';

export interface AddServerFormProps {
  initialLink?: string;
  onSuccess?: () => void;
}

export function AddServerForm({ initialLink, onSuccess }: AddServerFormProps) {
  const { colors } = useTheme();
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

  if (trustPending !== null) {
    return (
      <View style={[styles.sheet, { backgroundColor: colors.chat }]}>
        <TrustStep
          address={trustPending.address}
          fingerprint={trustPending.fingerprint}
          busy={busy}
          onTrust={confirmTrust}
          onCancel={cancelTrust}
        />
      </View>
    );
  }

  return (
    <KeyboardAvoidingView behavior="padding" style={[styles.sheet, { backgroundColor: colors.chat }]}>
      <View style={styles.content}>
        <Text variant="title2">Add a server</Text>
        <Text variant="callout" color="textSecondary">
          Enter an invite link or a server address.
        </Text>

        <View style={styles.inputGroup}>
          <View style={[styles.inputRow, { backgroundColor: colors.surface }]}>
            <TextInput
              value={linkOrAddress}
              onChangeText={setLinkOrAddress}
              placeholder="Invite link or address"
              placeholderTextColor={colors.textMuted}
              autoCapitalize="none"
              autoCorrect={false}
              keyboardType="url"
              returnKeyType="done"
              onSubmitEditing={() => void submit()}
              maxFontSizeMultiplier={MAX_FONT_SCALE}
              accessibilityLabel="Invite link or address"
              style={[typography.body, styles.input, { color: colors.text }]}
            />
            <Touchable
              onPress={() => void paste()}
              style={[styles.pasteButton, { backgroundColor: colors.elevated }]}
            >
              <View accessible accessibilityRole="button" accessibilityLabel="Paste from clipboard">
                <Text variant="footnote" color="textSecondary">
                  Paste
                </Text>
              </View>
            </Touchable>
          </View>

          {error !== null && (
            <Text variant="footnote" color="recording" accessibilityRole="alert" style={styles.errorText}>
              {error}
            </Text>
          )}
        </View>

        <Touchable
          onPress={() => setShowClaimToken((prev) => !prev)}
          style={styles.toggleRow}
        >
          <View
            accessible
            accessibilityRole="button"
            accessibilityLabel="I own this server"
            style={styles.toggleContent}
          >
            <Text variant="subhead" color="textSecondary">
              I own this server
            </Text>
            <View style={showClaimToken ? undefined : styles.collapsedIcon}>
              <Icon
                name={showClaimToken ? 'jumpDown' : 'back'}
                size={14}
                color="textSecondary"
              />
            </View>
          </View>
        </Touchable>

        {showClaimToken && (
          <View style={[styles.inputRow, { backgroundColor: colors.surface }]}>
            <TextInput
              value={claimToken}
              onChangeText={setClaimToken}
              placeholder="Claim token"
              placeholderTextColor={colors.textMuted}
              autoCapitalize="none"
              autoCorrect={false}
              returnKeyType="done"
              onSubmitEditing={() => void submit()}
              maxFontSizeMultiplier={MAX_FONT_SCALE}
              accessibilityLabel="Claim token"
              style={[typography.body, styles.input, { color: colors.text }]}
            />
          </View>
        )}

        <View style={styles.footer}>
          <Button title="Add" variant="primary" loading={busy} onPress={() => void submit()} />
        </View>
      </View>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  sheet: {
    flex: 1,
    paddingTop: space.lg,
  },
  content: {
    flex: 1,
    paddingHorizontal: space.lg,
    gap: space.md,
  },
  inputGroup: {
    gap: space.xs,
  },
  inputRow: {
    flexDirection: 'row',
    alignItems: 'center',
    borderRadius: radius.icon,
    paddingHorizontal: space.md,
    paddingVertical: space.xs,
  },
  input: {
    flex: 1,
    paddingVertical: space.sm,
  },
  pasteButton: {
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    borderRadius: radius.pill,
  },
  errorText: {
    paddingHorizontal: space.xs,
  },
  toggleRow: {
    paddingVertical: space.xs,
  },
  toggleContent: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  collapsedIcon: {
    transform: [{ rotate: '180deg' }],
  },
  footer: {
    marginTop: space.sm,
  },
});
