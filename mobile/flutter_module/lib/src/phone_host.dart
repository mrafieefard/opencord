import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:opencord_blocks/opencord_blocks.dart';
import 'package:opencord_plugin_host/opencord_plugin_host.dart';

import 'channel.dart';

/// What the renderer asks of the phone app (plugins M4 plan, Task 9). Icons
/// and rich text are drawn here; the rest are host calls React Native
/// answers. A call that fails or times out gives the block its fallback
/// (no image, no font, no file, the link not opened); nothing throws into
/// the renderer. Names and users are asked once per id, per area.
class PhoneBlocksHost implements BlocksHost {
  PhoneBlocksHost({required this.channel, required this.theme});

  final AreaChannel channel;

  /// Replaced when the setup changes; the caches stay.
  BlocksTheme theme;

  final _users = <String, Future<_User>>{};
  final _userNames = <String, String?>{};
  final _channelNames = <String, String?>{};
  final _namesAsked = <String>{};
  final _namesChanged = ValueNotifier<int>(0);

  Future<Map<String, Object?>?> _call(String method, Object? args, {Duration? wait = hostCallTimeout}) async {
    try {
      final reply = await channel.hostCall(method, args, wait: wait);
      return reply is Map ? Map<String, Object?>.from(reply) : <String, Object?>{};
    } on HostCallError catch (error) {
      debugPrint('plugin area: $method failed: ${error.kind}');
      return null;
    }
  }

  @override
  IconData? icon(String name) => pluginIconGlyphs[name];

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async {
    final path = (await _call('image', {'source': source}))?['path'];
    return path is String ? FileImage(File(path)) : null;
  }

  @override
  Future<ByteData?> font(String file) async {
    final path = (await _call('asset', {'path': 'assets/fonts/$file'}))?['path'];
    if (path is! String) return null;
    try {
      return ByteData.sublistView(await File(path).readAsBytes());
    } on FileSystemException {
      return null;
    }
  }

  @override
  Widget user(String id, double size, bool showName) => _UserView(host: this, id: id, size: size, showName: showName);

  Future<_User> _user(String id) => _users[id] ??= _call('user', {'id': id}).then((reply) {
    final name = reply?['name'];
    final avatar = reply?['avatar_path'];
    return _User(name is String && name.isNotEmpty ? name : 'User $id', avatar is String ? avatar : null);
  });

  @override
  Widget richText(String markdown, TextStyle style) => _RichText(host: this, markdown: markdown, style: style);

  /// Asks for the names of the mentions not asked yet.
  void _askNames(Set<String> users, Set<String> channels) {
    final newUsers = users.where((id) => _namesAsked.add('u$id')).toList();
    final newChannels = channels.where((id) => _namesAsked.add('c$id')).toList();
    if (newUsers.isEmpty && newChannels.isEmpty) return;
    unawaited(
      _call('names', {'users': newUsers, 'channels': newChannels}).then((reply) {
        final users = reply?['users'];
        final channels = reply?['channels'];
        for (final id in newUsers) {
          _userNames[id] = users is Map ? users[id] as String? : null;
        }
        for (final id in newChannels) {
          _channelNames[id] = channels is Map ? channels[id] as String? : null;
        }
        _namesChanged.value++;
      }),
    );
  }

  @override
  Future<String?> pickFile({required bool image, List<String> accept = const []}) async {
    // The viewer may take their time: no timeout.
    final path = (await _call('pick', {'image': image, 'accept': accept}, wait: null))?['path'];
    return path is String ? path : null;
  }

  @override
  Future<bool> confirmLink(String url) async => (await _call('link', {'url': url}))?['opened'] == true;

  @override
  void copy(String text) => unawaited(_call('copy', {'text': text}));

  @override
  void openChannel(String id) => unawaited(_call('open_channel', {'id': id}));

  @override
  Future<void> playSound(String asset, double volume, VoidCallback onEnd) async {
    // Resolves when the sound ends, or fails; onEnd runs either way.
    try {
      await _call('sound', {'asset': asset, 'volume': volume}, wait: null);
    } finally {
      onEnd();
    }
  }

  @override
  Future<void> upload(int nodeId, String path) async {
    await _call('upload', {'node_id': nodeId, 'path': path}, wait: null);
  }
}

class _User {
  const _User(this.name, this.avatarPath);
  final String name;
  final String? avatarPath;
}

class _UserView extends StatelessWidget {
  const _UserView({required this.host, required this.id, required this.size, required this.showName});

  final PhoneBlocksHost host;
  final String id;
  final double size;
  final bool showName;

  @override
  Widget build(BuildContext context) {
    final theme = host.theme;
    return FutureBuilder<_User>(
      future: host._user(id),
      builder: (context, snapshot) {
        final user = snapshot.data;
        final name = user?.name ?? '';
        final avatar = user?.avatarPath;
        final circle = ClipOval(
          child: SizedBox.square(
            dimension: size,
            child: avatar != null
                ? Image.file(File(avatar), fit: BoxFit.cover)
                : ColoredBox(
                    color: theme.colors['selected']!,
                    child: Center(
                      child: Text(
                        name.isEmpty ? '' : name.characters.first.toUpperCase(),
                        style: theme.text.copyWith(fontSize: size * 0.42, fontWeight: FontWeight.w600),
                      ),
                    ),
                  ),
          ),
        );
        if (!showName) return circle;
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            circle,
            const SizedBox(width: 8),
            Text(name, style: theme.text),
          ],
        );
      },
    );
  }
}

class _RichText extends StatefulWidget {
  const _RichText({required this.host, required this.markdown, required this.style});

  final PhoneBlocksHost host;
  final String markdown;
  final TextStyle style;

  @override
  State<_RichText> createState() => _RichTextState();
}

class _RichTextState extends State<_RichText> {
  @override
  void initState() {
    super.initState();
    widget.host._namesChanged.addListener(_changed);
    _ask();
  }

  @override
  void didUpdateWidget(_RichText old) {
    super.didUpdateWidget(old);
    if (old.markdown != widget.markdown) _ask();
  }

  @override
  void dispose() {
    widget.host._namesChanged.removeListener(_changed);
    super.dispose();
  }

  void _changed() => setState(() {});

  void _ask() {
    final users = {for (final m in userMentionToken.allMatches(widget.markdown)) m.group(1)!};
    final channels = {for (final m in channelMentionToken.allMatches(widget.markdown)) m.group(1)!};
    widget.host._askNames(users, channels);
  }

  @override
  Widget build(BuildContext context) {
    final host = widget.host;
    return MarkdownText(
      markdown: widget.markdown,
      style: widget.style,
      linkColor: host.theme.colors['accent']!,
      codeBackground: host.theme.colors['elevated']!,
      userName: (id) => host._userNames['$id'],
      channelName: (id) => host._channelNames['$id'],
      onLink: (url) => unawaited(host.confirmLink(url)),
    );
  }
}
