import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/widgets/avatar.dart';

/// The file of a profile picture or server icon, by its hash on a server;
/// null while it can't be fetched, and initials show instead.
final avatarFileProvider = FutureProvider.autoDispose
    .family<String?, ({String server, String hash})>((ref, picture) async {
      try {
        return await ref
            .read(repositoryProvider)
            .avatarPath(picture.server, picture.hash);
      } on RepoException {
        return null;
      }
    });

/// The picture named [hash] on [server], once its file is here; null
/// without one.
ImageProvider? avatarImage(WidgetRef ref, String server, String? hash) {
  if (hash == null) return null;
  final path = ref
      .watch(avatarFileProvider((server: server, hash: hash)))
      .value;
  return path == null ? null : FileImage(File(path));
}

/// Someone's profile picture on [serverKey], or their initials.
class MemberAvatar extends ConsumerWidget {
  const MemberAvatar({
    super.key,
    required this.serverKey,
    required this.userId,
    required this.name,
    this.size = 36,
    this.presence,
    this.speaking = false,
    this.ringColor,
  });

  final String serverKey;
  final int userId;
  final String name;
  final double size;
  final Presence? presence;
  final bool speaking;
  final Color? ringColor;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final hash = ref.watch(
      serverProvider(serverKey).select((state) {
        final data = state.data;
        return data?.members[userId]?.user.avatarHash ??
            (data?.self.id == userId ? data?.self.avatarHash : null);
      }),
    );
    return OcAvatar(
      id: userId,
      name: name,
      size: size,
      presence: presence,
      speaking: speaking,
      ringColor: ringColor,
      image: avatarImage(ref, serverKey, hash),
    );
  }
}

/// A server's icon, or its initials.
class ServerAvatar extends ConsumerWidget {
  const ServerAvatar({
    super.key,
    required this.serverKey,
    required this.name,
    this.size = 36,
    this.borderRadius = 9,
  });

  final String serverKey;
  final String name;
  final double size;
  final double borderRadius;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final hash = ref.watch(
      serverProvider(serverKey).select((state) => state.data?.info.iconHash),
    );
    return OcAvatar(
      id: serverKey,
      name: name,
      size: size,
      borderRadius: borderRadius,
      image: avatarImage(ref, serverKey, hash),
    );
  }
}
