import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/private.dart';

/// Adapts a [PrivateMessage] to the chat [Message] model (§6.3).
Message asChatMessage(PrivateMessage m) {
  final hexPrefix = m.id.length >= 16
      ? m.id.substring(0, 16)
      : m.id.padRight(16, '0');
  final intId =
      BigInt.tryParse(hexPrefix, radix: 16)?.toSigned(64).toInt() ??
      m.id.hashCode;

  return Message(
    id: intId,
    channelId: 0,
    authorId: m.outgoing ? -1 : -2,
    content: m.content,
    createdAt: m.sentAt,
    editedAt: m.editedAt,
    sendState: switch (m.state) {
      PrivateSendState.sending => SendState.pending,
      PrivateSendState.failed => SendState.failed,
      _ => SendState.sent,
    },
    attachments: const [],
  );
}
