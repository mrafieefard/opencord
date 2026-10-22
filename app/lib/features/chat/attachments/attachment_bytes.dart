import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/repository/repository.dart';

typedef AttachmentKey = ({String server, int id, bool thumbnail});

final attachmentBytesProvider = FutureProvider.autoDispose
    .family<Uint8List, AttachmentKey>((ref, key) async {
      final repo = ref.watch(repositoryProvider);
      return repo.attachmentBytes(key.server, key.id, thumbnail: key.thumbnail);
    });
