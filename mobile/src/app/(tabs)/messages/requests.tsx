import { memo, useEffect } from 'react';
import { FlatList, StyleSheet, View } from 'react-native';
import { router } from 'expo-router';
import type { PrivateContactRecord } from '@opencord/core-native';

import { report } from '@/core';
import { acceptMessageRequest, ignoreMessageRequest, loadPrivate } from '@/state/actions/private';
import { usePrivateContacts } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { Avatar, Button, EmptyState, PaneHeader, Text, space, useTheme } from '@/ui';
import { privateName } from '@/features/private/privateAdapter';

export const RequestsScreen = memo(function RequestsScreen() {
  const { colors } = useTheme();
  const contactsLoaded = useAppStore((state) => state.private.loaded);
  useEffect(() => {
    if (!contactsLoaded) report(loadPrivate(), 'load private chats');
  }, [contactsLoaded]);
  const requests = usePrivateContacts('requests');

  const handleAccept = (peer: string) => {
    report(acceptMessageRequest(peer), 'accept message request');
    router.push(`/(tabs)/messages/${peer}` as never);
  };

  const handleDecline = (peer: string) => {
    report(ignoreMessageRequest(peer), 'decline message request');
  };

  const renderItem = ({ item }: { item: PrivateContactRecord }) => {
    const name = privateName(item, item.peer);
    const messageText = item.preview && item.preview.length > 0 ? item.preview : 'Wants to message you';

    return (
      <View style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]} testID={`request-card-${item.peer}`}>
        <View style={styles.cardHeader}>
          <Avatar name={name} size={44} />
          <View style={styles.cardInfo}>
            <Text variant="headline" numberOfLines={1}>
              {name}
            </Text>
            <Text variant="subhead" color="textSecondary" numberOfLines={2} style={styles.cardPreview}>
              {messageText}
            </Text>
          </View>
        </View>
        <View style={styles.cardActions}>
          <View style={styles.buttonSlot} testID={`accept-request-${item.peer}`}>
            <Button
              title="Accept"
              variant="primary"
              onPress={() => handleAccept(item.peer)}
            />
          </View>
          <View style={styles.buttonSlot} testID={`decline-request-${item.peer}`}>
            <Button
              title="Decline"
              variant="secondary"
              onPress={() => handleDecline(item.peer)}
            />
          </View>
        </View>
      </View>
    );
  };

  return (
    <View style={styles.container}>
      <PaneHeader
        title="Message requests"
        left={{ icon: 'back', label: 'Back', onPress: () => router.back() }}
      />
      {requests.length > 0 && (
        <View style={styles.headerNotice}>
          <Text variant="subhead" color="textSecondary">
            They wrote first. Nothing reaches your chats until you accept.
          </Text>
        </View>
      )}
      {requests.length === 0 ? (
        <EmptyState
          title="Message requests"
          body="No one is waiting for an answer"
        />
      ) : (
        <FlatList
          data={requests}
          keyExtractor={(item) => item.peer}
          renderItem={renderItem}
          contentContainerStyle={styles.listContent}
        />
      )}
    </View>
  );
});

export default RequestsScreen;

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  headerNotice: {
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
  },
  listContent: {
    padding: space.lg,
    gap: space.md,
  },
  card: {
    padding: space.md,
    borderRadius: 14,
    borderWidth: StyleSheet.hairlineWidth,
  },
  cardHeader: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  cardInfo: {
    flex: 1,
    marginLeft: space.md,
  },
  cardPreview: {
    marginTop: space.xs,
  },
  cardActions: {
    flexDirection: 'row',
    marginTop: space.md,
    gap: space.sm,
  },
  buttonSlot: {
    flex: 1,
  },
});
