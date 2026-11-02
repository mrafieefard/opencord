// Download in the media viewer (D86): straight into the downloads folder,
// as Telegram's save does, never over a file already there.

import 'dart:io';
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/features/chat/attachments/download.dart';

const _clip = Attachment(
  id: 701,
  filename: 'clip.mp4',
  contentType: 'video/mp4',
  sizeBytes: 1000,
  kind: AttachmentKind.video,
);

void main() {
  test('download_saves_beside_a_file_of_the_same_name', () async {
    final folder = await Directory.systemTemp.createTemp('downloads');
    addTearDown(() => folder.delete(recursive: true));
    File('${folder.path}/clip.mp4').writeAsStringSync('mine');

    final saved = await saveToDownloads(
      MockRepository(random: Random(1), simulateLife: false),
      'server.example:7710',
      _clip,
      downloads: () async => folder,
    );

    expect(saved, '${folder.path}${Platform.pathSeparator}clip (2).mp4');
    expect(File('${folder.path}/clip.mp4').readAsStringSync(), 'mine');
  });

  test('no_downloads_folder_answers_null', () async {
    final saved = await saveToDownloads(
      MockRepository(random: Random(1), simulateLife: false),
      'server.example:7710',
      _clip,
      downloads: () async => null,
    );
    expect(saved, isNull);
  });
}
