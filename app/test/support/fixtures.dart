import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/user.dart';

const serverKey = 'chat.example.org:7710';
const selfId = 1000;
const kaiId = 1001;
const miraId = 1002;
const everyoneRoleId = 1;
const generalId = 10;
const randomId = 11;

final t0 = DateTime.utc(2026, 10, 7, 12);

User user(int id, String name) => User(id: id, displayName: name);

Member member(int id, String name, {List<int> roles = const []}) =>
    Member(user: user(id, name), joinedAt: t0, roleIds: roles);

Message message(
  int id, {
  int channelId = generalId,
  int authorId = kaiId,
  String content = 'hello',
  String? nonce,
  int minute = 0,
}) => Message(
  id: id,
  channelId: channelId,
  authorId: authorId,
  content: content,
  createdAt: t0.add(Duration(minutes: minute)),
  nonce: nonce,
);

ReadySnapshot snapshot({
  Map<int, Message> lastMessages = const {},
  Map<int, ReadState> readStates = const {},
}) => ReadySnapshot(
  self: user(selfId, 'Alex'),
  info: const ServerInfo(
    name: 'Opencord Dev',
    everyoneRoleId: everyoneRoleId,
    ownerId: selfId,
  ),
  channels: const [
    Channel(id: generalId, kind: ChannelKind.text, name: 'general'),
    Channel(id: randomId, kind: ChannelKind.text, name: 'random'),
  ],
  roles: const [
    Role(
      id: everyoneRoleId,
      name: '@everyone',
      position: 0,
      permissions: Permissions.defaultEveryone,
    ),
    Role(
      id: 2,
      name: 'Maintainer',
      position: 1,
      permissions: Permissions.none,
      hoist: true,
    ),
  ],
  members: [
    member(selfId, 'Alex'),
    member(kaiId, 'Kai', roles: [2]),
    member(miraId, 'Mira'),
  ],
  presences: const {selfId: Presence.online, kaiId: Presence.idle},
  activities: const {kaiId: 'Editing main.rs'},
  serverPermissions: Permissions.all,
  channelPermissions: const {
    generalId: Permissions.all,
    randomId: Permissions.all,
  },
  lastMessages: lastMessages,
  readStates: readStates,
);
