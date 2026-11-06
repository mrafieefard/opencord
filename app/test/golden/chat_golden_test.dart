import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/chat_rows.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/ui/widgets/avatar.dart';

import '../support/pump.dart';

const _me = 1000;
const _kai = 1001;
const _mira = 1002;
final _at = DateTime(2026, 10, 7, 9, 41);

final _links = MarkdownContext(
  userName: (id) => const {_me: 'Alex', _kai: 'Kai', _mira: 'Mira'}[id],
  channelName: (id) => const {10: 'general'}[id],
);

Message _message(
  int id,
  int author,
  String content, {
  bool edited = false,
  SendState state = SendState.sent,
  List<Reaction> reactions = const [],
}) => Message(
  id: id,
  channelId: 10,
  authorId: author,
  content: content,
  createdAt: _at.add(Duration(minutes: id)),
  editedAt: edited ? _at : null,
  reactions: reactions,
  sendState: state,
);

Widget _compact(
  Message message, {
  required bool first,
  String author = 'Kai',
  String? role,
  ReplyPreview? reply,
  bool mentionsMe = false,
}) => CompactMessage(
  serverKey: 'golden',
  message: message,
  first: first,
  author: author,
  avatar: first ? OcAvatar(id: author, name: author, size: 36) : null,
  role: role,
  reply: reply,
  mentionsMe: mentionsMe,
  links: _links,
);

void main() {
  for (final (name, colors) in themes) {
    testWidgets('compact lines ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(640, 520),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            children: [
              const CenterPill(text: 'Today'),
              _compact(
                _message(
                  1,
                  _kai,
                  'Morning! Has anyone tried the **new build**?',
                ),
                first: true,
                role: 'Maintainer',
              ),
              _compact(
                _message(
                  2,
                  _kai,
                  'It has the `cargo test` fixes.',
                  edited: true,
                ),
                first: false,
              ),
              _compact(
                _message(3, _kai, '```rust\nfn main() {\n    run();\n}\n```'),
                first: false,
              ),
              _compact(
                _message(
                  4,
                  _mira,
                  '<@$_me> can you review <#10>?',
                  reactions: const [
                    Reaction(emoji: '👍', userIds: [_me, _kai], me: true),
                  ],
                ),
                first: true,
                author: 'Mira',
                reply: const ReplyPreview(
                  author: 'Kai',
                  text: 'Morning! Has anyone tried the new build?',
                ),
                mentionsMe: true,
              ),
              _compact(
                _message(
                  5,
                  _me,
                  'Sure, looking now.',
                  state: SendState.pending,
                ),
                first: true,
                author: 'Alex',
              ),
              _compact(
                _message(
                  6,
                  _me,
                  'This one did not go out.',
                  state: SendState.failed,
                ),
                first: false,
                author: 'Alex',
              ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/compact_$name.png'),
      );
    });
  }
}
