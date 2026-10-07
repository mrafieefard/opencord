import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/channel_audience.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/mentions.dart';
import 'package:opencord/ui/emoji/emoji.dart';

/// One line of the composer's autocomplete (§4.6).
sealed class Suggestion {
  const Suggestion();

  /// What replaces the typed token.
  String get insert;
}

final class MemberSuggestion extends Suggestion {
  const MemberSuggestion(this.member, {this.role});

  final Member member;
  final String? role;

  @override
  String get insert => '@${member.displayName} ';
}

final class ChannelSuggestion extends Suggestion {
  const ChannelSuggestion(this.channel);

  final Channel channel;

  @override
  String get insert => '#${channel.name} ';
}

final class EmojiSuggestion extends Suggestion {
  const EmojiSuggestion(this.emoji);

  final Emoji emoji;

  @override
  String get insert => '${emoji.char} ';
}

const maxSuggestions = 8;

/// 0 when [name] starts with [query], 1 when one of its words does.
int? _rank(String name, String query) {
  final lower = name.toLowerCase();
  if (lower.startsWith(query)) return 0;
  if (lower.split(RegExp(r'[\s\-_]')).any((word) => word.startsWith(query))) {
    return 1;
  }
  return null;
}

List<T> _best<T>(
  Iterable<T> items,
  String Function(T item) name,
  String query,
) {
  final ranked = [
    for (final item in items)
      if (_rank(name(item), query) case final rank?) (rank, item),
  ];
  ranked.sort((a, b) {
    if (a.$1 != b.$1) return a.$1 - b.$1;
    return name(a.$2).toLowerCase().compareTo(name(b.$2).toLowerCase());
  });
  return [for (final (_, item) in ranked.take(maxSuggestions)) item];
}

/// Suggestions for [token] in [channel]: members who can see the channel,
/// channels the reader can see, or emoji.
List<Suggestion> suggest(ServerData data, Channel channel, ActiveToken token) {
  final query = token.query.toLowerCase();
  switch (token.trigger) {
    case '@':
      return [
        for (final member in _best(
          channelViewers(data, channel),
          (member) => member.displayName,
          query,
        ))
          MemberSuggestion(member, role: _role(data, member)),
      ];
    case '#':
      final visible = data.channels.values.where(
        (candidate) =>
            candidate.kind.isTextLike &&
            data.permissionsIn(candidate.id).has(Permissions.viewChannel),
      );
      return [
        for (final found in _best(visible, (c) => c.name, query))
          ChannelSuggestion(found),
      ];
    case ':':
      return [
        for (final emoji in searchEmoji(token.query, limit: maxSuggestions))
          EmojiSuggestion(emoji),
      ];
  }
  return const [];
}

String? _role(ServerData data, Member member) {
  final role = data.highestRole(member);
  return role == null || role.id == data.info.everyoneRoleId ? null : role.name;
}
