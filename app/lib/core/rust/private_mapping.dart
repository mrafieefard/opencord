import 'package:opencord/core/model/private.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

FriendStatus friendStatusFrom(core.FriendStatus status) => switch (status) {
  core.FriendStatus.none => FriendStatus.none,
  core.FriendStatus.outgoing => FriendStatus.outgoing,
  core.FriendStatus.incoming => FriendStatus.incoming,
  core.FriendStatus.friends => FriendStatus.friends,
};

core.FriendStatus friendStatusTo(FriendStatus status) => switch (status) {
  FriendStatus.none => core.FriendStatus.none,
  FriendStatus.outgoing => core.FriendStatus.outgoing,
  FriendStatus.incoming => core.FriendStatus.incoming,
  FriendStatus.friends => core.FriendStatus.friends,
};

ConversationStatus conversationStatusFrom(core.ConversationStatus status) =>
    switch (status) {
      core.ConversationStatus.none => ConversationStatus.none,
      core.ConversationStatus.request => ConversationStatus.request,
      core.ConversationStatus.accepted => ConversationStatus.accepted,
    };

core.ConversationStatus conversationStatusTo(ConversationStatus status) =>
    switch (status) {
      ConversationStatus.none => core.ConversationStatus.none,
      ConversationStatus.request => core.ConversationStatus.request,
      ConversationStatus.accepted => core.ConversationStatus.accepted,
    };

PrivateMessageKind privateMessageKindFrom(core.PrivateMessageKind kind) =>
    switch (kind) {
      core.PrivateMessageKind.text => PrivateMessageKind.text,
      core.PrivateMessageKind.divider => PrivateMessageKind.divider,
      core.PrivateMessageKind.moved => PrivateMessageKind.moved,
      core.PrivateMessageKind.undecryptable => PrivateMessageKind.undecryptable,
      core.PrivateMessageKind.keysChanged => PrivateMessageKind.keysChanged,
      core.PrivateMessageKind.newDevice => PrivateMessageKind.newDevice,
      core.PrivateMessageKind.unacceptedKeys =>
        PrivateMessageKind.unacceptedKeys,
    };

core.PrivateMessageKind privateMessageKindTo(PrivateMessageKind kind) =>
    switch (kind) {
      PrivateMessageKind.text => core.PrivateMessageKind.text,
      PrivateMessageKind.divider => core.PrivateMessageKind.divider,
      PrivateMessageKind.moved => core.PrivateMessageKind.moved,
      PrivateMessageKind.undecryptable => core.PrivateMessageKind.undecryptable,
      PrivateMessageKind.keysChanged => core.PrivateMessageKind.keysChanged,
      PrivateMessageKind.newDevice => core.PrivateMessageKind.newDevice,
      PrivateMessageKind.unacceptedKeys =>
        core.PrivateMessageKind.unacceptedKeys,
    };

PrivateVerification privateVerificationFrom(core.PrivateVerification value) =>
    switch (value) {
      core.PrivateVerification.unverified => PrivateVerification.unverified,
      core.PrivateVerification.verified => PrivateVerification.verified,
      core.PrivateVerification.mismatch => PrivateVerification.mismatch,
    };

OwnDevice ownDeviceFrom(core.OwnDevice device) => OwnDevice(
  deviceId: device.deviceId,
  state: switch (device.state) {
    core.OwnDeviceState.other => OwnDeviceState.other,
    core.OwnDeviceState.offering => OwnDeviceState.offering,
  },
);

PrivateSendState privateSendStateFrom(core.PrivateSendState state) =>
    switch (state) {
      core.PrivateSendState.received => PrivateSendState.received,
      core.PrivateSendState.sending => PrivateSendState.sending,
      core.PrivateSendState.sent => PrivateSendState.sent,
      core.PrivateSendState.failed => PrivateSendState.failed,
    };

core.PrivateSendState privateSendStateTo(PrivateSendState state) =>
    switch (state) {
      PrivateSendState.received => core.PrivateSendState.received,
      PrivateSendState.sending => core.PrivateSendState.sending,
      PrivateSendState.sent => core.PrivateSendState.sent,
      PrivateSendState.failed => core.PrivateSendState.failed,
    };

PrivateContact privateContactFrom(core.PrivateContact contact) =>
    PrivateContact(
      peer: contact.peer,
      name: contact.name,
      friend: friendStatusFrom(contact.friend),
      requestSpam: contact.requestSpam,
      blocked: contact.blocked,
      conversation: conversationStatusFrom(contact.conversation),
      relayServerKey: contact.relayServerKey,
      lastActivity: contact.lastActivityMs != null
          ? DateTime.fromMillisecondsSinceEpoch(
              contact.lastActivityMs!.toInt(),
              isUtc: true,
            )
          : null,
      unread: contact.unread,
      muted: contact.muted,
      hidden: contact.hidden,
      preview: contact.preview ?? '',
      verification: privateVerificationFrom(contact.verification),
      newKeys: contact.newKeys,
      reachable: contact.reachable,
    );

PrivateMessage privateMessageFrom(core.PrivateMessage message) =>
    PrivateMessage(
      id: message.id,
      peer: message.peer,
      outgoing: message.outgoing,
      kind: privateMessageKindFrom(message.kind),
      content: message.content,
      serverKey: message.serverKey,
      sentAt: DateTime.fromMillisecondsSinceEpoch(
        message.sentAtMs.toInt(),
        isUtc: true,
      ),
      editedAt: message.editedAtMs != null
          ? DateTime.fromMillisecondsSinceEpoch(
              message.editedAtMs!.toInt(),
              isUtc: true,
            )
          : null,
      state: privateSendStateFrom(message.state),
    );

PrivacySettings privacySettingsFrom(core.PrivacySettingsData data) =>
    PrivacySettings(
      allowFriendRequests: data.allowFriendRequests,
      allowMessageRequests: data.allowMessageRequests,
    );

core.PrivacySettingsData privacySettingsTo(PrivacySettings settings) =>
    core.PrivacySettingsData(
      allowFriendRequests: settings.allowFriendRequests,
      allowMessageRequests: settings.allowMessageRequests,
    );
