import { memo, useCallback, useEffect, useState } from 'react';
import { FlatList, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { router } from 'expo-router';
import { FriendStatus, type PrivateContactRecord } from '@opencord/core-native';

import { report } from '@/core';
import {
  answerFriendRequest,
  cancelFriendRequest,
  loadPrivate,
  unblockPeer,
} from '@/state/actions/private';
import { usePrivateContacts } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { Avatar, Button, EmptyState, PaneHeader, Segmented, Text, space, useTheme } from '@/ui';

import { privateName } from './privateAdapter';

const TABS = ['Online', 'All', 'Pending', 'Blocked'];

export const FriendsPage = memo(function FriendsPage() {
  const { colors } = useTheme();
  const [tabIndex, setTabIndex] = useState(0);

  const contactsLoaded = useAppStore((state) => state.private.loaded);
  useEffect(() => {
    if (!contactsLoaded) report(loadPrivate(), 'load private contacts');
  }, [contactsLoaded]);

  const friends = usePrivateContacts('friends');
  const pending = usePrivateContacts('pending');
  const blocked = usePrivateContacts('blocked');

  const onlineFriends = friends.filter((c) => c.reachable !== false);

  const incomingRequests = pending.filter((c) => c.friend === FriendStatus.Incoming && !c.requestSpam);
  const outgoingRequests = pending.filter((c) => c.friend === FriendStatus.Outgoing);
  const spamRequests = pending.filter((c) => c.friend === FriendStatus.Incoming && c.requestSpam);

  const handleOpenChat = useCallback((peer: string) => {
    router.push(`/(tabs)/messages/${peer}` as never);
  }, []);

  const renderFriendRow = (contact: PrivateContactRecord, subtitle: string) => {
    const name = privateName(contact, contact.peer);
    return (
      <Touchable
        key={contact.peer}
        testID={`friend-card-${contact.peer}`}
        style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]}
        onPress={() => handleOpenChat(contact.peer)}
      >
        <Avatar name={name} size={40} />
        <View style={styles.cardInfo}>
          <Text variant="headline" numberOfLines={1}>
            {name}
          </Text>
          <Text variant="callout" color="textSecondary" numberOfLines={1}>
            {subtitle}
          </Text>
        </View>
        <Button
          title="Message"
          variant="secondary"
          icon="messages"
          onPress={() => handleOpenChat(contact.peer)}
        />
      </Touchable>
    );
  };

  const renderContent = () => {
    switch (tabIndex) {
      case 0: {
        // Online tab
        if (onlineFriends.length === 0) {
          return <EmptyState title="Friends" body="No one's online right now." />;
        }
        return (
          <View style={styles.listContainer}>
            <Text variant="caption" color="textMuted" style={styles.sectionHeader}>
              {`ONLINE — ${onlineFriends.length}`}
            </Text>
            {onlineFriends.map((f) => renderFriendRow(f, 'Online'))}
          </View>
        );
      }

      case 1: {
        // All tab
        if (friends.length === 0) {
          return <EmptyState title="Friends" body="You haven't added any friends yet." />;
        }
        return (
          <View style={styles.listContainer}>
            <Text variant="caption" color="textMuted" style={styles.sectionHeader}>
              {`ALL — ${friends.length}`}
            </Text>
            {friends.map((f) => renderFriendRow(f, f.reachable !== false ? 'Online' : 'Offline'))}
          </View>
        );
      }

      case 2: {
        // Pending tab
        const totalNonSpam = incomingRequests.length + outgoingRequests.length;
        if (totalNonSpam === 0 && spamRequests.length === 0) {
          return <EmptyState title="Friends" body="No pending requests." />;
        }

        return (
          <View style={styles.listContainer}>
            {incomingRequests.length > 0 && (
              <View style={styles.section}>
                <Text variant="caption" color="textMuted" style={styles.sectionHeader}>
                  {`INCOMING — ${incomingRequests.length}`}
                </Text>
                {incomingRequests.map((req) => {
                  const name = privateName(req, req.peer);
                  return (
                    <View
                      key={req.peer}
                      testID={`incoming-card-${req.peer}`}
                      style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]}
                    >
                      <Avatar name={name} size={40} />
                      <View style={styles.cardInfo}>
                        <Text variant="headline" numberOfLines={1}>
                          {name}
                        </Text>
                        <Text variant="callout" color="textSecondary" numberOfLines={1}>
                          Wants to be your friend
                        </Text>
                      </View>
                      <View style={styles.actionRow}>
                        <Button
                          title="Accept"
                          variant="primary"
                          onPress={() => report(answerFriendRequest(req.peer, true), 'accept friend request')}
                        />
                        <Button
                          title="Decline"
                          variant="secondary"
                          onPress={() => report(answerFriendRequest(req.peer, false), 'decline friend request')}
                        />
                      </View>
                    </View>
                  );
                })}
              </View>
            )}

            {outgoingRequests.length > 0 && (
              <View style={styles.section}>
                <Text variant="caption" color="textMuted" style={styles.sectionHeader}>
                  {`SENT — ${outgoingRequests.length}`}
                </Text>
                {outgoingRequests.map((req) => {
                  const name = privateName(req, req.peer);
                  return (
                    <View
                      key={req.peer}
                      testID={`outgoing-card-${req.peer}`}
                      style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]}
                    >
                      <Avatar name={name} size={40} />
                      <View style={styles.cardInfo}>
                        <Text variant="headline" numberOfLines={1}>
                          {name}
                        </Text>
                        <Text variant="callout" color="textSecondary" numberOfLines={1}>
                          Friend request sent
                        </Text>
                      </View>
                      <Button
                        title="Cancel"
                        variant="secondary"
                        onPress={() => report(cancelFriendRequest(req.peer), 'cancel friend request')}
                      />
                    </View>
                  );
                })}
              </View>
            )}

            {spamRequests.length > 0 && (
              <View style={styles.section}>
                <Text variant="caption" color="textMuted" style={styles.sectionHeader}>
                  {`Spam — ${spamRequests.length}`}
                </Text>
                {spamRequests.map((req) => {
                  const name = privateName(req, req.peer);
                  return (
                    <View
                      key={req.peer}
                      testID={`spam-card-${req.peer}`}
                      style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]}
                    >
                      <Avatar name={name} size={40} />
                      <View style={styles.cardInfo}>
                        <Text variant="headline" numberOfLines={1}>
                          {name}
                        </Text>
                        <Text variant="callout" color="textSecondary" numberOfLines={1}>
                          Wants to be your friend
                        </Text>
                      </View>
                      <View style={styles.actionRow}>
                        <Button
                          title="Accept"
                          variant="primary"
                          onPress={() => report(answerFriendRequest(req.peer, true), 'accept friend request')}
                        />
                        <Button
                          title="Decline"
                          variant="secondary"
                          onPress={() => report(answerFriendRequest(req.peer, false), 'decline friend request')}
                        />
                      </View>
                    </View>
                  );
                })}
              </View>
            )}
          </View>
        );
      }

      case 3: {
        // Blocked tab
        if (blocked.length === 0) {
          return <EmptyState title="Friends" body="You haven't blocked anyone." />;
        }

        return (
          <View style={styles.listContainer}>
            <Text variant="caption" color="textMuted" style={styles.sectionHeader}>
              {`BLOCKED — ${blocked.length}`}
            </Text>
            {blocked.map((b) => {
              const name = privateName(b, b.peer);
              return (
                <View
                  key={b.peer}
                  testID={`blocked-card-${b.peer}`}
                  style={[styles.card, { backgroundColor: colors.surface, borderColor: colors.border }]}
                >
                  <Avatar name={name} size={40} />
                  <View style={styles.cardInfo}>
                    <Text variant="headline" numberOfLines={1}>
                      {name}
                    </Text>
                    <Text variant="callout" color="textSecondary" numberOfLines={1}>
                      Blocked
                    </Text>
                  </View>
                  <Button
                    title="Unblock"
                    variant="secondary"
                    onPress={() => report(unblockPeer(b.peer), 'unblock peer')}
                  />
                </View>
              );
            })}
          </View>
        );
      }

      default:
        return null;
    }
  };

  return (
    <View style={[styles.container, { backgroundColor: colors.chat }]}>
      <PaneHeader title="Friends" left={{ icon: 'back', label: 'Back', onPress: () => router.back() }} />
      <View style={styles.segmentedWrapper}>
        <Segmented
          values={TABS}
          index={tabIndex}
          onChange={setTabIndex}
        />
      </View>
      <FlatList
        data={[{ key: 'content' }]}
        keyExtractor={(item) => item.key}
        renderItem={renderContent}
        contentContainerStyle={styles.scrollContent}
      />
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  segmentedWrapper: {
    paddingHorizontal: space.md,
    paddingVertical: space.sm,
  },
  scrollContent: {
    padding: space.md,
    flexGrow: 1,
  },
  listContainer: {
    gap: space.md,
  },
  section: {
    gap: space.sm,
  },
  sectionHeader: {
    fontWeight: '700',
    letterSpacing: 0.8,
    marginVertical: space.xs,
  },
  card: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: space.md,
    borderRadius: 12,
    borderWidth: 1,
    gap: space.md,
  },
  cardInfo: {
    flex: 1,
    gap: 2,
  },
  actionRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
  },
});
