import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// An `opencord://` link (§15): an invite, or a channel and maybe one of
/// its messages.
@immutable
sealed class AppLink {
  const AppLink();
}

/// `opencord://host:port/invite/CODE#fp=…`, opened in the Add server
/// dialog, which reads the rest.
final class InviteAppLink extends AppLink {
  const InviteAppLink(this.link);

  final String link;

  @override
  bool operator ==(Object other) =>
      other is InviteAppLink && other.link == link;

  @override
  int get hashCode => link.hashCode;
}

/// `opencord://host:port/c/<channel>[/<message>]`, what "Copy link" makes.
final class ChannelAppLink extends AppLink {
  const ChannelAppLink(this.server, this.channel, [this.message]);

  /// The server key, `host:port`.
  final String server;
  final int channel;
  final int? message;

  @override
  bool operator ==(Object other) =>
      other is ChannelAppLink &&
      other.server == server &&
      other.channel == channel &&
      other.message == message;

  @override
  int get hashCode => Object.hash(server, channel, message);
}

/// The link in [text], or null when it is not one the app opens.
AppLink? parseAppLink(String text) {
  final trimmed = text.trim();
  final uri = Uri.tryParse(trimmed);
  if (uri == null || !uri.isScheme('opencord') || uri.host.isEmpty) {
    return null;
  }
  final server = uri.hasPort ? '${uri.host}:${uri.port}' : uri.host;
  switch (uri.pathSegments) {
    case ['invite', final code] when code.isNotEmpty:
      return InviteAppLink(trimmed);
    case ['c', final channel]:
      final id = int.tryParse(channel);
      return id == null ? null : ChannelAppLink(server, id);
    case ['c', final channel, final message]:
      final id = int.tryParse(channel);
      final messageId = int.tryParse(message);
      return id == null || messageId == null
          ? null
          : ChannelAppLink(server, id, messageId);
    default:
      return null;
  }
}

/// Opens [link] in the app: an invite in the Add server dialog (or the
/// server, when it is already added), a channel or message link in place.
Future<void> openAppLink(
  BuildContext context,
  WidgetRef ref,
  AppLink link,
) async {
  final servers = ref.read(serverListProvider);
  final navigation = ref.read(navigationProvider.notifier);
  switch (link) {
    case InviteAppLink(link: final invite):
      final key = parseAppLinkServer(invite);
      final known = servers.where((server) => server.key == key).firstOrNull;
      if (known == null) return showAddServer(context, link: invite);
      navigation.openServer(known.key);
      showOcToast(context, 'You are already in ${known.name}.');
    case ChannelAppLink(:final server, :final channel, :final message):
      if (!servers.any((summary) => summary.key == server)) {
        showOcToast(context, 'You are not in that server.');
        return;
      }
      final channels = ref.read(serverProvider(server)).data?.channels;
      if (channels != null && !channels.containsKey(channel)) {
        showOcToast(context, 'That channel is gone, or hidden from you.');
        return;
      }
      final chat = ref.read(chatControllerProvider);
      final open =
          ref.read(currentServerProvider) == server &&
          ref.read(currentChannelProvider) == channel;
      if (message != null && open) {
        await chat.jumpToMessage(message);
        return;
      }
      if (message != null) {
        chat.jumpWhenOpened((server: server, channel: channel), message);
      }
      navigation.openChannel(server, channel);
  }
}

/// The `host:port` of an `opencord://` link.
String? parseAppLinkServer(String link) {
  final uri = Uri.tryParse(link.trim());
  if (uri == null || uri.host.isEmpty) return null;
  return uri.hasPort ? '${uri.host}:${uri.port}' : uri.host;
}

/// Links from the command line and from later launches (§15), waiting for
/// the shell to open them.
class AppLinkInbox extends Notifier<List<String>> {
  @override
  List<String> build() {
    final later = ref.watch(nativeWindowProvider).links.listen(add);
    ref.onDispose(later.cancel);
    return const [];
  }

  void add(String link) => state = [...state, link];

  List<String> takeAll() {
    final links = state;
    if (links.isNotEmpty) state = const [];
    return links;
  }
}

final appLinkInboxProvider = NotifierProvider<AppLinkInbox, List<String>>(
  AppLinkInbox.new,
);
