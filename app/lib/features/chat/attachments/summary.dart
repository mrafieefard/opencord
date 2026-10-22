import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';

/// Formats a single-line summary of a message for replies, pins, and notifications.
String messageSummary(Message m) {
  if (m.content.trim().isNotEmpty) {
    return m.content;
  }
  if (m.attachments.isEmpty) {
    return '';
  }
  final images = m.attachments
      .where((a) => a.kind == AttachmentKind.image)
      .toList();
  if (images.length == m.attachments.length) {
    return images.length == 1 ? 'Photo' : '${images.length} photos';
  }
  final videos = m.attachments
      .where((a) => a.kind == AttachmentKind.video)
      .toList();
  if (videos.isNotEmpty) {
    return 'Video';
  }
  final voice = m.attachments
      .where((a) => a.kind == AttachmentKind.voiceMessage)
      .toList();
  if (voice.isNotEmpty) {
    final ms = voice.first.durationMs ?? 0;
    final totalSeconds = (ms / 1000).round();
    final mins = totalSeconds ~/ 60;
    final secs = (totalSeconds % 60).toString().padLeft(2, '0');
    return 'Voice message ($mins:$secs)';
  }
  final first = m.attachments.first.filename;
  if (m.attachments.length == 1) {
    return first;
  }
  return '$first and ${m.attachments.length - 1} more';
}
