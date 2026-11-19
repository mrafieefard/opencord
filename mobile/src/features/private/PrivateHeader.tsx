// A private chat's header (design §6.3, the desktop's private_header.dart):
// their avatar and name, the chat's encryption state, and a ⋯ menu.
import { FriendStatus, PrivateVerification, type PrivateContactRecord } from '@opencord/core-native';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { report } from '@/core';
import { blockPeer, removeFriend, setPrivateMuted } from '@/state/actions/private';
import { Avatar, Icon, Menu, PaneHeader, confirm, type ColorToken, type IconName, type MenuAction } from '@/ui';

import { BLOCK_MESSAGE } from './RequestBar';

export interface PrivateHeaderProps {
  peer: string;
  contact: PrivateContactRecord | undefined;
  name: string;
  /** The back button; none beside the list on a tablet. */
  onBack?: () => void;
  /** Opens the encryption sheet (Task 8); without it the lock only shows the state. */
  onOpenEncryption?: () => void;
}

/** The lock's look and what it says, the desktop's words. */
export function encryptionState(
  verification: PrivateVerification,
  name: string,
): { icon: IconName; color: ColorToken; label: string } {
  switch (verification) {
    case PrivateVerification.Verified:
      return { icon: 'shield', color: 'text', label: `End-to-end encrypted. Verified with ${name}.` };
    case PrivateVerification.Mismatch:
      return { icon: 'warning', color: 'text', label: "End-to-end encrypted. The words didn't match." };
    default:
      return {
        icon: 'lock',
        color: 'textSecondary',
        label: `End-to-end encrypted. Not verified: compare the words with ${name}.`,
      };
  }
}

export function privateMenuActions(contact: PrivateContactRecord | undefined): MenuAction[] {
  const actions: MenuAction[] = [
    contact?.muted
      ? { id: 'unmute', title: 'Unmute notifications', icon: 'volume' }
      : { id: 'mute', title: 'Mute notifications', icon: 'volumeOff' },
  ];
  if (!contact?.blocked) actions.push({ id: 'block', title: 'Block', icon: 'close', destructive: true });
  if (contact?.friend === FriendStatus.Friends) {
    actions.push({ id: 'removeFriend', title: 'Remove friend', icon: 'delete', destructive: true });
  }
  return actions;
}

export function PrivateHeader({ peer, contact, name, onBack, onOpenEncryption }: PrivateHeaderProps) {
  const lock = encryptionState(contact?.verification ?? PrivateVerification.Unverified, name);

  const onAction = (id: string) => {
    switch (id) {
      case 'mute':
        report(setPrivateMuted(peer, true), 'mute');
        break;
      case 'unmute':
        report(setPrivateMuted(peer, false), 'unmute');
        break;
      case 'block':
        confirm({
          title: `Block ${name}?`,
          message: BLOCK_MESSAGE,
          confirmTitle: 'Block',
          destructive: true,
          onConfirm: () => report(blockPeer(peer), 'block'),
        });
        break;
      case 'removeFriend':
        confirm({
          title: `Remove ${name}?`,
          message: `Are you sure you want to remove ${name} as a friend?`,
          confirmTitle: 'Remove friend',
          destructive: true,
          onConfirm: () => report(removeFriend(peer), 'remove friend'),
        });
        break;
    }
  };

  const lockIcon = <Icon name={lock.icon} size={20} color={lock.color} />;

  return (
    <PaneHeader
      title={name}
      titleLeading={<Avatar name={name} size={28} />}
      left={onBack ? { icon: 'back', label: 'Back', onPress: onBack } : undefined}
      rightElement={
        <>
          {onOpenEncryption ? (
            <Touchable onPress={onOpenEncryption} style={styles.control}>
              <View style={styles.center} accessible accessibilityRole="button" accessibilityLabel={lock.label} testID="private-lock">
                {lockIcon}
              </View>
            </Touchable>
          ) : (
            <View style={styles.control} accessible accessibilityLabel={lock.label} testID="private-lock">
              {lockIcon}
            </View>
          )}
          <Menu actions={privateMenuActions(contact)} onAction={onAction} testID="private-menu">
            <View style={styles.control} accessible accessibilityRole="button" accessibilityLabel="More">
              <Icon name="more" size={22} color="text" />
            </View>
          </Menu>
        </>
      }
    />
  );
}

const styles = StyleSheet.create({
  control: { width: 44, height: 44, alignItems: 'center', justifyContent: 'center' },
  center: { flex: 1, alignSelf: 'stretch', alignItems: 'center', justifyContent: 'center' },
});
