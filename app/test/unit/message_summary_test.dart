import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/summary.dart';

Message _msg({String content = '', List<Attachment> attachments = const []}) =>
    Message(
      id: 1,
      channelId: 10,
      authorId: 100,
      content: content,
      createdAt: DateTime(2026, 1, 1),
      attachments: attachments,
    );

Attachment _att({
  required int id,
  required String filename,
  required AttachmentKind kind,
  int? durationMs,
}) => Attachment(
  id: id,
  filename: filename,
  contentType: 'application/octet-stream',
  sizeBytes: 1024,
  kind: kind,
  durationMs: durationMs,
);

void main() {
  group('messageSummary', () {
    test('non-empty content returns content', () {
      final m = _msg(
        content: 'Hello world',
        attachments: [
          _att(id: 1, filename: 'cat.png', kind: AttachmentKind.image),
        ],
      );
      expect(messageSummary(m), 'Hello world');
    });

    test('one image returns Photo', () {
      final m = _msg(
        content: '',
        attachments: [
          _att(id: 1, filename: 'cat.png', kind: AttachmentKind.image),
        ],
      );
      expect(messageSummary(m), 'Photo');
    });

    test('multiple images return count photos', () {
      final m = _msg(
        content: '',
        attachments: [
          _att(id: 1, filename: 'cat.png', kind: AttachmentKind.image),
          _att(id: 2, filename: 'dog.png', kind: AttachmentKind.image),
          _att(id: 3, filename: 'bird.png', kind: AttachmentKind.image),
        ],
      );
      expect(messageSummary(m), '3 photos');
    });

    test('video returns Video', () {
      final m = _msg(
        content: '',
        attachments: [
          _att(id: 1, filename: 'clip.mp4', kind: AttachmentKind.video),
        ],
      );
      expect(messageSummary(m), 'Video');
    });

    test('voice message returns Voice message (m:ss)', () {
      final m = _msg(
        content: '',
        attachments: [
          _att(
            id: 1,
            filename: 'voice.ogg',
            kind: AttachmentKind.voiceMessage,
            durationMs: 65000,
          ),
        ],
      );
      expect(messageSummary(m), 'Voice message (1:05)');
    });

    test('single file returns its filename', () {
      final m = _msg(
        content: '',
        attachments: [
          _att(id: 1, filename: 'report.pdf', kind: AttachmentKind.file),
        ],
      );
      expect(messageSummary(m), 'report.pdf');
    });

    test('multiple files return first filename and count more', () {
      final m = _msg(
        content: '',
        attachments: [
          _att(id: 1, filename: 'report.pdf', kind: AttachmentKind.file),
          _att(id: 2, filename: 'notes.txt', kind: AttachmentKind.file),
          _att(id: 3, filename: 'data.csv', kind: AttachmentKind.file),
        ],
      );
      expect(messageSummary(m), 'report.pdf and 2 more');
    });
  });
}
