import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/private.dart';
import 'package:opencord/core/rust/private_mapping.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

void main() {
  group('enums map both ways', () {
    test('FriendStatus', () {
      for (final value in FriendStatus.values) {
        final coreValue = friendStatusTo(value);
        expect(friendStatusFrom(coreValue), value);
      }
      for (final coreValue in core.FriendStatus.values) {
        final appValue = friendStatusFrom(coreValue);
        expect(friendStatusTo(appValue), coreValue);
      }
    });

    test('ConversationStatus', () {
      for (final value in ConversationStatus.values) {
        final coreValue = conversationStatusTo(value);
        expect(conversationStatusFrom(coreValue), value);
      }
      for (final coreValue in core.ConversationStatus.values) {
        final appValue = conversationStatusFrom(coreValue);
        expect(conversationStatusTo(appValue), coreValue);
      }
    });

    test('PrivateMessageKind', () {
      for (final value in PrivateMessageKind.values) {
        final coreValue = privateMessageKindTo(value);
        expect(privateMessageKindFrom(coreValue), value);
      }
      for (final coreValue in core.PrivateMessageKind.values) {
        final appValue = privateMessageKindFrom(coreValue);
        expect(privateMessageKindTo(appValue), coreValue);
      }
    });

    test('PrivateSendState', () {
      for (final value in PrivateSendState.values) {
        final coreValue = privateSendStateTo(value);
        expect(privateSendStateFrom(coreValue), value);
      }
      for (final coreValue in core.PrivateSendState.values) {
        final appValue = privateSendStateFrom(coreValue);
        expect(privateSendStateTo(appValue), coreValue);
      }
    });
  });

  group('models and nulls', () {
    test('PrivateContact with all values and with nulls', () {
      final fullCore = core.PrivateContact(
        peer: 'aabbcc',
        fingerprint: 'AABB-CCDD-EEFF-0011',
        name: 'Kai',
        friend: core.FriendStatus.friends,
        requestSpam: false,
        blocked: false,
        conversation: core.ConversationStatus.accepted,
        relayServerKey: 'relay.example:7710',
        lastActivityMs: 1760000000000,
        unread: 3,
        muted: true,
        hidden: false,
        preview: 'See you tomorrow!',
      );

      final full = privateContactFrom(fullCore);
      expect(full.peer, 'aabbcc');
      expect(full.fingerprint, 'AABB-CCDD-EEFF-0011');
      expect(full.name, 'Kai');
      expect(full.friend, FriendStatus.friends);
      expect(full.requestSpam, isFalse);
      expect(full.blocked, isFalse);
      expect(full.conversation, ConversationStatus.accepted);
      expect(full.relayServerKey, 'relay.example:7710');
      expect(
        full.lastActivity,
        DateTime.fromMillisecondsSinceEpoch(1760000000000, isUtc: true),
      );
      expect(full.unread, 3);
      expect(full.muted, isTrue);
      expect(full.hidden, isFalse);
      expect(full.preview, 'See you tomorrow!');

      const nullCore = core.PrivateContact(
        peer: 'ddeeff',
        fingerprint: 'DDEE-FF00-1122-3344',
        name: null,
        friend: core.FriendStatus.none,
        requestSpam: false,
        blocked: true,
        conversation: core.ConversationStatus.none,
        relayServerKey: null,
        lastActivityMs: null,
        unread: 0,
        muted: false,
        hidden: true,
        preview: null,
      );

      final nullMapped = privateContactFrom(nullCore);
      expect(nullMapped.name, isNull);
      expect(nullMapped.relayServerKey, isNull);
      expect(nullMapped.lastActivity, isNull);
      expect(nullMapped.preview, '');
    });

    test('PrivateMessage with all values and with nulls', () {
      const textCore = core.PrivateMessage(
        id: 'msg1',
        peer: 'peer1',
        outgoing: true,
        kind: core.PrivateMessageKind.text,
        content: 'Hello world',
        serverKey: null,
        sentAtMs: 1760000000000,
        editedAtMs: 1760000010000,
        state: core.PrivateSendState.sent,
      );

      final text = privateMessageFrom(textCore);
      expect(text.id, 'msg1');
      expect(text.peer, 'peer1');
      expect(text.outgoing, isTrue);
      expect(text.kind, PrivateMessageKind.text);
      expect(text.content, 'Hello world');
      expect(text.serverKey, isNull);
      expect(
        text.sentAt,
        DateTime.fromMillisecondsSinceEpoch(1760000000000, isUtc: true),
      );
      expect(
        text.editedAt,
        DateTime.fromMillisecondsSinceEpoch(1760000010000, isUtc: true),
      );
      expect(text.state, PrivateSendState.sent);

      const dividerCore = core.PrivateMessage(
        id: 'div1',
        peer: 'peer1',
        outgoing: false,
        kind: core.PrivateMessageKind.divider,
        content: 'Opencord Dev',
        serverKey: 'opencord.example:7710',
        sentAtMs: 1759999999000,
        editedAtMs: null,
        state: core.PrivateSendState.received,
      );

      final divider = privateMessageFrom(dividerCore);
      expect(divider.kind, PrivateMessageKind.divider);
      expect(divider.serverKey, 'opencord.example:7710');
      expect(divider.editedAt, isNull);
    });

    test('PrivacySettings maps both ways', () {
      const open = PrivacySettings(
        allowFriendRequests: true,
        allowMessageRequests: true,
      );
      final openCore = privacySettingsTo(open);
      expect(openCore.allowFriendRequests, isTrue);
      expect(openCore.allowMessageRequests, isTrue);
      expect(privacySettingsFrom(openCore), open);

      const closed = PrivacySettings(
        allowFriendRequests: false,
        allowMessageRequests: false,
      );
      final closedCore = privacySettingsTo(closed);
      expect(closedCore.allowFriendRequests, isFalse);
      expect(closedCore.allowMessageRequests, isFalse);
      expect(privacySettingsFrom(closedCore), closed);
    });
  });
}
