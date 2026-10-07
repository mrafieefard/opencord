import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/user.dart';

/// Snowflake-like ids: newer is larger, and two ids made in the same
/// millisecond still differ.
class MockIds {
  int _sequence = 0;

  int at(DateTime time) =>
      time.millisecondsSinceEpoch * 1024 + (_sequence++ & 1023);
}

/// One server as the mock knows it. Unlike the UI state, it holds every
/// channel and message, and it changes in place.
class MockServer {
  MockServer({
    required this.key,
    required this.info,
    required this.selfId,
    required this.fingerprint,
    this.unreachable = false,
  });

  final String key;
  ServerInfo info;
  final int selfId;

  /// Lowercase hex SHA-256 of the (pretend) certificate.
  final String fingerprint;

  /// Never connects; shows its cached state while it keeps retrying.
  bool unreachable;

  final Map<int, Channel> channels = {};
  final Map<int, Role> roles = {};
  final Map<int, Member> members = {};
  final Map<int, Presence> presences = {};
  final Map<int, String> activities = {};

  /// Oldest first, per channel.
  final Map<int, List<Message>> messages = {};
  final Map<int, List<VoiceParticipant>> voice = {};
  final Map<int, ReadState> readStates = {};
  final List<Invite> invites = [];
  final List<Ban> bans = [];

  ServerSummary get summary => ServerSummary(
    key: key,
    name: info.name,
    fingerprint: fingerprint,
    userId: selfId,
  );

  Member get self => members[selfId]!;

  PermissionContext contextOf(int userId) {
    final member = members[userId];
    return PermissionContext(
      userId: userId,
      isOwner: info.ownerId == userId,
      everyoneRoleId: info.everyoneRoleId,
      everyone: roles[info.everyoneRoleId]!.permissions,
      roles: [
        for (final id in member?.roleIds ?? const <int>[])
          if (roles[id] case final role?)
            RoleGrant(
              id: role.id,
              position: role.position,
              permissions: role.permissions,
            ),
      ],
    );
  }

  Permissions get selfPermissions => contextOf(selfId).base;

  /// The current user's permissions in every channel they can view.
  Map<int, Permissions> get selfChannelPermissions {
    final context = contextOf(selfId);
    return {
      for (final channel in channels.values)
        if (context.inChannel(channel.overwrites) case final resolved
            when resolved.has(Permissions.viewChannel))
          channel.id: resolved,
    };
  }

  ReadySnapshot snapshot() {
    final visible = selfChannelPermissions;
    return ReadySnapshot(
      self: self.user,
      info: info,
      channels: [
        for (final channel in channels.values)
          if (visible.containsKey(channel.id)) channel,
      ],
      roles: roles.values.toList(),
      members: members.values.toList(),
      presences: Map.of(presences),
      activities: Map.of(activities),
      serverPermissions: selfPermissions,
      channelPermissions: visible,
      voice: {
        for (final MapEntry(:key, :value) in voice.entries)
          if (visible.containsKey(key)) key: List.of(value),
      },
      lastMessages: {
        for (final MapEntry(:key, :value) in messages.entries)
          if (visible.containsKey(key) && value.isNotEmpty) key: value.last,
      },
      readStates: {
        for (final MapEntry(:key, :value) in readStates.entries)
          if (visible.containsKey(key)) key: value,
      },
    );
  }
}

class MockWorld {
  MockWorld(this.servers, this.ids);

  final List<MockServer> servers;
  final MockIds ids;
}

const selfName = 'Alex Rivera';
const selfFingerprint = 'KQ7M-3XPA-ZR2D-W9TB';

/// Builds the three servers of plan §11 around [now].
MockWorld buildMockWorld(DateTime now) {
  final ids = MockIds();
  return MockWorld([
    _opencordDev(now, ids),
    _rustBerlin(now, ids),
    _homelab(now, ids),
  ], ids);
}

String fakeFingerprint(String seed) {
  var hash = 0x811c9dc5;
  final buffer = StringBuffer();
  for (var i = 0; buffer.length < 64; i++) {
    hash =
        ((hash ^ seed.codeUnitAt(i % seed.length) ^ i) * 0x01000193) &
        0xffffffff;
    buffer.write(hash.toRadixString(16).padLeft(8, '0'));
  }
  return buffer.toString().substring(0, 64);
}

String _keyFingerprint(int seed) {
  const alphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
  var value = seed * 2654435761 & 0xffffffffffff;
  final chars = <String>[];
  for (var i = 0; i < 16; i++) {
    chars.add(alphabet[value % alphabet.length]);
    value = (value ~/ alphabet.length) ^ (seed * (i + 7));
    value &= 0xffffffffffff;
  }
  return [
    for (var i = 0; i < 16; i += 4) chars.sublist(i, i + 4).join(),
  ].join('-');
}

/// Fills a [MockServer] with channels, people and history.
class _Builder {
  _Builder(this.server, this.now, this.ids, {required this.channelBase});

  final MockServer server;
  final DateTime now;
  final MockIds ids;

  /// Channel ids start here, so they stay the same from run to run.
  final int channelBase;
  int _channelId = 0;

  DateTime get todayStart => DateTime(now.year, now.month, now.day);

  DateTime daysAgo(int days, int hour, int minute) =>
      DateTime(now.year, now.month, now.day - days, hour, minute);

  /// A moment earlier today, [fraction] of the way from the start of the
  /// window to just before now.
  DateTime today(double fraction) {
    final start = now.subtract(const Duration(hours: 6)).isAfter(todayStart)
        ? now.subtract(const Duration(hours: 6))
        : todayStart;
    final end = now.subtract(const Duration(minutes: 2)).isAfter(start)
        ? now.subtract(const Duration(minutes: 2))
        : now;
    final span = end.difference(start).inMilliseconds;
    return start.add(Duration(milliseconds: (span * fraction).round()));
  }

  void role(
    int id,
    String name,
    int position,
    Permissions permissions, {
    bool hoist = false,
  }) {
    server.roles[id] = Role(
      id: id,
      name: name,
      position: position,
      permissions: permissions,
      hoist: hoist,
      mentionable: hoist,
    );
  }

  void person(
    int id,
    String name, {
    List<int> roles = const [],
    Presence presence = Presence.online,
    String? activity,
    int joinedDaysAgo = 40,
  }) {
    server.members[id] = Member(
      user: User(
        id: id,
        displayName: name,
        fingerprint: id == server.selfId
            ? selfFingerprint
            : _keyFingerprint(id),
        publicKeyHex: fakeFingerprint('key-$id'),
      ),
      roleIds: roles,
      joinedAt: daysAgo(joinedDaysAgo, 10, 0),
    );
    server.presences[id] = presence;
    if (activity != null) server.activities[id] = activity;
  }

  int category(String name) => channel(name, ChannelKind.category);

  int channel(
    String name,
    ChannelKind kind, {
    int? parent,
    String? topic,
    List<PermissionOverwrite> overwrites = const [],
  }) {
    final id = channelBase + ++_channelId;
    final position = server.channels.values
        .where(
          (c) =>
              c.parentId == parent &&
              c.isCategory == (kind == ChannelKind.category),
        )
        .length;
    server.channels[id] = Channel(
      id: id,
      kind: kind,
      name: name,
      topic: topic,
      parentId: parent,
      position: position,
      overwrites: overwrites,
    );
    if (kind.isTextLike) server.messages[id] = [];
    if (kind == ChannelKind.voice) server.voice[id] = [];
    return id;
  }

  PermissionOverwrite everyoneDeny(Permissions deny) => PermissionOverwrite(
    targetKind: OverwriteTargetKind.role,
    targetId: server.info.everyoneRoleId,
    allow: Permissions.none,
    deny: deny,
  );

  PermissionOverwrite roleAllow(int roleId, Permissions allow) =>
      PermissionOverwrite(
        targetKind: OverwriteTargetKind.role,
        targetId: roleId,
        allow: allow,
        deny: Permissions.none,
      );

  Message say(
    int channelId,
    int authorId,
    DateTime at,
    String content, {
    Message? replyTo,
    Map<String, List<int>> reactions = const {},
    bool pinned = false,
    Duration? editedAfter,
    SystemEvent? system,
  }) {
    final message = Message(
      id: ids.at(at),
      channelId: channelId,
      authorId: authorId,
      content: content,
      createdAt: at,
      editedAt: editedAfter == null ? null : at.add(editedAfter),
      replyToId: replyTo?.id,
      reactions: [
        for (final MapEntry(:key, :value) in reactions.entries)
          Reaction(
            emoji: key,
            userIds: value,
            me: value.contains(server.selfId),
          ),
      ],
      pinned: pinned,
      systemEvent: system,
    );
    server.messages[channelId]!.add(message);
    return message;
  }

  /// Marks everything up to and including [message] as read, and counts
  /// what came after it.
  void readUpTo(int channelId, Message? message) {
    final all = server.messages[channelId]!;
    final lastReadId = message?.id ?? 0;
    final after = all.where(
      (m) => m.id > lastReadId && m.authorId != server.selfId,
    );
    server.readStates[channelId] = ReadState(
      lastReadId: lastReadId,
      unread: after.length,
      mentions: after
          .where((m) => m.content.contains('<@${server.selfId}>'))
          .length,
    );
  }

  void readAll(int channelId) {
    final all = server.messages[channelId]!;
    readUpTo(channelId, all.isEmpty ? null : all.last);
  }

  void inVoice(int channelId, List<VoiceParticipant> participants) {
    server.voice[channelId] = List.of(participants);
  }
}

MockServer _opencordDev(DateTime now, MockIds ids) {
  const self = 1000, kai = 1001, mira = 1002, jonas = 1003, lena = 1004;
  const priya = 1005, tomas = 1006, ava = 1007, noah = 1008, sofia = 1009;
  const ethan = 1010, yuki = 1011, omar = 1012;
  const everyone = 1, contributor = 2, maintainer = 3;
  final server = MockServer(
    key: 'opencord.example:7710',
    info: const ServerInfo(
      serverIdHex: '6f70656e636f72642d6465762d303031',
      name: 'Opencord Dev',
      description: 'Building a self-hostable chat, one milestone at a time.',
      ownerId: self,
      everyoneRoleId: everyone,
    ),
    selfId: self,
    fingerprint: fakeFingerprint('opencord.example:7710'),
  );
  final b = _Builder(server, now, ids, channelBase: 10000);
  b.role(everyone, '@everyone', 0, Permissions.defaultEveryone);
  b.role(contributor, 'Contributor', 1, Permissions.createInvite, hoist: true);
  b.role(
    maintainer,
    'Maintainer',
    2,
    Permissions.manageMessages |
        Permissions.manageChannels |
        Permissions.kickMembers |
        Permissions.manageNicknames |
        Permissions.mentionEveryone,
    hoist: true,
  );

  b.person(self, selfName, roles: [maintainer], joinedDaysAgo: 120);
  b.person(
    kai,
    'Kai Nakamura',
    roles: [maintainer],
    activity: 'Editing main.rs',
  );
  b.person(
    mira,
    'Mira Okafor',
    roles: [maintainer],
    activity: 'In voice · General',
  );
  b.person(
    jonas,
    'Jonas Weber',
    roles: [contributor],
    presence: Presence.idle,
    activity: 'In voice · General',
  );
  b.person(
    lena,
    'Lena Fischer',
    roles: [contributor],
    presence: Presence.doNotDisturb,
    activity: 'Focusing',
  );
  b.person(
    priya,
    'Priya Shah',
    roles: [contributor],
    activity: 'In voice · General',
    joinedDaysAgo: 1,
  );
  b.person(tomas, 'Tomás Ruiz', activity: 'In voice · General');
  b.person(ava, 'Ava Lindqvist', presence: Presence.idle);
  b.person(noah, 'Noah Kim', presence: Presence.offline);
  b.person(sofia, 'Sofia Rossi', presence: Presence.offline);
  b.person(ethan, 'Ethan Brooks', activity: 'Listening to lo-fi');
  b.person(
    yuki,
    'Yuki Tanaka',
    roles: [contributor],
    presence: Presence.doNotDisturb,
  );
  b.person(omar, 'Omar Haddad', presence: Presence.offline);

  final information = b.category('Information');
  final announcements = b.channel(
    'announcements',
    ChannelKind.announcement,
    parent: information,
    topic: 'Releases and milestones',
    overwrites: [
      b.everyoneDeny(Permissions.sendMessages),
      b.roleAllow(maintainer, Permissions.sendMessages),
    ],
  );
  final welcome = b.channel(
    'welcome',
    ChannelKind.text,
    parent: information,
    topic: 'Start here',
    overwrites: [b.everyoneDeny(Permissions.sendMessages)],
  );
  final text = b.category('Text channels');
  final general = b.channel(
    'general',
    ChannelKind.text,
    parent: text,
    topic: 'Everything Opencord, one message at a time',
  );
  final devCore = b.channel(
    'dev-core',
    ChannelKind.text,
    parent: text,
    topic: 'Rust core, protocol and server',
  );
  final design = b.channel(
    'design-review',
    ChannelKind.text,
    parent: text,
    topic: 'Mockups, tokens and the widget gallery',
  );
  final help = b.channel(
    'help',
    ChannelKind.text,
    parent: text,
    topic: 'Ask anything about running a server',
  );
  final offTopic = b.channel('off-topic', ChannelKind.text, parent: text);
  final maintainers = b.channel(
    'maintainers',
    ChannelKind.text,
    parent: text,
    topic: 'Release planning',
    overwrites: [
      b.everyoneDeny(Permissions.viewChannel),
      b.roleAllow(maintainer, Permissions.viewChannel),
    ],
  );
  final voice = b.category('Voice');
  final generalVoice = b.channel('General', ChannelKind.voice, parent: voice);
  b.channel('Pairing', ChannelKind.voice, parent: voice);
  b.channel('Late night', ChannelKind.voice, parent: voice);

  b.inVoice(generalVoice, const [
    VoiceParticipant(userId: mira),
    VoiceParticipant(userId: tomas, screensharing: true),
    VoiceParticipant(userId: priya, camera: true),
    VoiceParticipant(userId: jonas, muted: true),
  ]);

  b.say(
    welcome,
    kai,
    b.daysAgo(30, 10, 0),
    'Welcome to **Opencord Dev**! Read <#$general> for the day-to-day, <#$devCore> for the Rust side and <#$design> for the UI.\n\nBe kind, keep it on topic, and ask in <#$help> when you are stuck.',
  );
  b.readAll(welcome);

  b.say(
    announcements,
    kai,
    b.daysAgo(1, 11, 0),
    '**Opencord 0.1** is tagged: server, client core and the bridge. Thanks everyone 🎉',
    reactions: {
      '🎉': [mira, jonas, priya, self],
    },
  );
  final lastAnnouncementRead = server.messages[announcements]!.last;
  b.say(
    announcements,
    kai,
    b.today(0.1),
    'M5 starts today: the desktop app. Follow along in <#$design>.',
  );
  b.readUpTo(announcements, lastAnnouncementRead);

  // #general: three days ago, yesterday and today.
  b.say(
    general,
    noah,
    b.daysAgo(3, 16, 20),
    'Is there a Docker image yet, or should I build from source?',
  );
  b.say(
    general,
    kai,
    b.daysAgo(3, 16, 32),
    'From source for now. A compose file lands with M7.',
  );
  b.say(
    general,
    priya,
    b.daysAgo(1, 9, 12),
    '',
    system: SystemEvent.memberJoined,
  );
  final handshake = b.say(
    general,
    kai,
    b.daysAgo(1, 9, 30),
    'Morning! I pushed the gateway handshake to `pre`. Reviews welcome.',
  );
  b.say(
    general,
    kai,
    b.daysAgo(1, 9, 31),
    'It binds the Identify signature to the TLS certificate now, so a relayed login fails.',
  );
  final resumeQuestion = b.say(
    general,
    mira,
    b.daysAgo(1, 9, 42),
    'Nice, reading it now. Does resume keep a replay buffer per session?',
    replyTo: handshake,
  );
  b.say(
    general,
    kai,
    b.daysAgo(1, 9, 44),
    'Yes: 1 000 events or 60 seconds, whichever comes first.',
    replyTo: resumeQuestion,
  );
  b.say(
    general,
    jonas,
    b.daysAgo(1, 10, 5),
    "Here's the backoff I'd use for reconnects:\n```rust\nlet delay = base * 2u32.pow(attempt.min(5));\nlet jitter = delay.mul_f32(rng.gen_range(0.75..1.25));\nsleep(jitter.min(Duration::from_secs(30))).await;\n```",
    reactions: {
      '👍': [kai, mira],
    },
  );
  b.say(
    general,
    mira,
    b.daysAgo(1, 10, 7),
    '**Design review** at 15:00: the new rail and the two-line channel rows.',
  );
  b.say(
    general,
    lena,
    b.daysAgo(1, 15, 2),
    'Mockups are up. Rows have two lines now, Telegram style, with the last message under the name.',
  );
  b.say(
    general,
    lena,
    b.daysAgo(1, 15, 3),
    'And everything stays monochrome: presence is a shape, not a color.',
  );
  final labels = b.say(
    general,
    self,
    b.daysAgo(1, 15, 20),
    'Love it. Can we keep the server names under the rail icons?',
  );
  b.say(
    general,
    lena,
    b.daysAgo(1, 15, 21),
    'Yes, 11 px and one line.',
    replyTo: labels,
  );
  b.say(
    general,
    ethan,
    b.daysAgo(1, 18, 40),
    '_Quick reminder:_ the Berlin meetup is on Thursday.',
  );
  b.say(
    general,
    tomas,
    b.daysAgo(1, 21, 15),
    '~~Monday~~ Tuesday works better for the voice prototype.',
  );
  b.say(
    general,
    kai,
    b.today(0.05),
    'Release checklist for M5:\n1. Tokens and theme\n2. Widget gallery\n3. Mock data\n4. Shell and shortcuts',
    pinned: true,
  );
  b.say(general, kai, b.today(0.06), '', system: SystemEvent.messagePinned);
  final invitePing = b.say(
    general,
    priya,
    b.today(0.2),
    '<@$self> could you look at the invite dialog copy when you have a minute?',
    reactions: {
      '👀': [mira],
    },
  );
  b.say(
    general,
    self,
    b.today(0.25),
    'On it. The fingerprint note needs to be shorter.',
    replyTo: invitePing,
    reactions: {
      '👍': [kai, priya],
    },
  );
  b.say(
    general,
    mira,
    b.today(0.4),
    'Has anyone tried the quick switcher yet? Ctrl K jumps between servers now.',
    editedAfter: const Duration(minutes: 1),
  );
  final quote = b.say(
    general,
    kai,
    b.today(0.45),
    '> Ctrl K jumps between servers now\nAnd to members too, with prefix matches first.',
  );
  b.say(
    general,
    jonas,
    b.today(0.6),
    'Pushed the reconnect fix. `cargo test` is green.',
    reactions: {
      '🎉': [kai, mira, self],
    },
  );
  b.say(general, yuki, b.today(0.7), 'Could someone look at <#$design> later?');
  b.say(
    general,
    ethan,
    b.today(0.8),
    'Meetup details: https://meetup.example/rust-berlin',
  );
  b.say(general, priya, b.today(0.95), 'Thanks <@$self>! Shorter is better.');
  b.readUpTo(general, quote);

  b.say(
    devCore,
    kai,
    b.daysAgo(1, 13, 0),
    'The core now reconnects with exponential backoff and tries Resume first.',
  );
  final resumeRead = b.say(
    devCore,
    jonas,
    b.daysAgo(1, 13, 4),
    'Kicks close with 4010 and stop retrying, right?',
  );
  b.say(devCore, kai, b.today(0.3), 'Right. Bans close with 4011.');
  b.say(
    devCore,
    omar,
    b.today(0.5),
    'Should a changed certificate stop retries too?',
  );
  b.say(
    devCore,
    kai,
    b.today(0.55),
    'Yes, and it shows the fingerprint mismatch dialog.',
  );
  b.readUpTo(devCore, resumeRead);

  final designRead = b.say(
    design,
    lena,
    b.daysAgo(1, 14, 50),
    'Tokens are final: rail, sidebar, chat, surface, elevated.',
  );
  b.say(
    design,
    lena,
    b.today(0.35),
    '<@$self> the theme cards need a miniature of each theme. Can you take that?',
  );
  b.readUpTo(design, designRead);

  b.say(
    help,
    sofia,
    b.daysAgo(2, 19, 0),
    'My server prints a claim token. What do I do with it?',
  );
  b.say(
    help,
    kai,
    b.daysAgo(2, 19, 5),
    'Paste it under "I\'m the owner" when you add the server. It works once.',
  );
  b.readAll(help);

  b.say(
    offTopic,
    ava,
    b.daysAgo(2, 12, 0),
    'Coffee or tea while reviewing PRs?',
  );
  b.say(offTopic, ethan, b.daysAgo(2, 12, 3), 'Mate, obviously.');
  b.say(offTopic, tomas, b.daysAgo(2, 12, 9), 'Water and regret.');
  b.readAll(offTopic);

  b.say(maintainers, mira, b.daysAgo(1, 17, 0), 'Shall we cut 0.2 after M6?');
  b.say(
    maintainers,
    self,
    b.daysAgo(1, 17, 5),
    'Yes, once the settings pages land.',
  );
  b.readAll(maintainers);
  return server;
}

MockServer _rustBerlin(DateTime now, MockIds ids) {
  const self = 2000, greta = 2001, lukas = 2002, kai = 2003, marta = 2004;
  const felix = 2005, elif = 2006, paul = 2007, hannah = 2008;
  const everyone = 20, speaker = 21, organizer = 22;
  final server = MockServer(
    key: 'rust-berlin.example:7710',
    info: const ServerInfo(
      serverIdHex: '727573742d6265726c696e2d30303031',
      name: 'Rust Berlin',
      description: 'Monthly meetups, talks and hack nights.',
      ownerId: greta,
      everyoneRoleId: everyone,
      openJoin: true,
    ),
    selfId: self,
    fingerprint: fakeFingerprint('rust-berlin.example:7710'),
  );
  final b = _Builder(server, now, ids, channelBase: 20000);
  b.role(everyone, '@everyone', 0, Permissions.defaultEveryone);
  b.role(speaker, 'Speaker', 1, Permissions.none, hoist: true);
  b.role(
    organizer,
    'Organizer',
    2,
    Permissions.manageMessages |
        Permissions.manageChannels |
        Permissions.kickMembers |
        Permissions.banMembers |
        Permissions.manageServer,
    hoist: true,
  );
  b.person(self, selfName, joinedDaysAgo: 60);
  b.person(greta, 'Greta Hoffmann', roles: [organizer]);
  b.person(lukas, 'Lukas Braun', roles: [organizer], presence: Presence.idle);
  b.person(kai, 'Kai Nakamura', roles: [speaker], activity: 'Preparing slides');
  b.person(marta, 'Marta Nowak', roles: [speaker], presence: Presence.offline);
  b.person(felix, 'Felix Schulz', activity: 'In voice · Lounge');
  b.person(elif, 'Elif Demir', activity: 'In voice · Lounge');
  b.person(paul, 'Paul Becker', presence: Presence.offline);
  b.person(hannah, 'Hannah Vogel', presence: Presence.doNotDisturb);

  final meetups = b.category('Meetups');
  final announcements = b.channel(
    'announcements',
    ChannelKind.announcement,
    parent: meetups,
    overwrites: [
      b.everyoneDeny(Permissions.sendMessages),
      b.roleAllow(organizer, Permissions.sendMessages),
    ],
  );
  final general = b.channel(
    'general',
    ChannelKind.text,
    parent: meetups,
    topic: 'Say hi, ask anything',
  );
  final jobs = b.channel('jobs', ChannelKind.text, parent: meetups);
  final showAndTell = b.channel(
    'show-and-tell',
    ChannelKind.text,
    parent: meetups,
  );
  final lounge = b.channel('Lounge', ChannelKind.voice, parent: meetups);
  b.inVoice(lounge, const [
    VoiceParticipant(userId: elif),
    VoiceParticipant(userId: felix, muted: true),
  ]);

  final announcementRead = b.say(
    announcements,
    greta,
    b.daysAgo(6, 18, 0),
    'October meetup: talks on async traits and embedded Rust.',
  );
  b.say(
    announcements,
    greta,
    b.today(0.2),
    'Doors open at 18:30 on Thursday. See you there!',
  );
  b.readUpTo(announcements, announcementRead);

  final read = b.say(
    general,
    lukas,
    b.daysAgo(1, 20, 0),
    'Who is bringing the projector cable?',
  );
  b.say(general, felix, b.today(0.1), 'I can, if someone reminds me.');
  b.say(
    general,
    kai,
    b.today(0.3),
    'Slides for the async talk are almost done.',
  );
  b.say(general, elif, b.today(0.5), 'Is there a waitlist this time?');
  b.say(general, greta, b.today(0.6), 'Yes, 20 people so far.');
  b.say(general, hannah, b.today(0.9), 'Count me in for the hack night.');
  b.readUpTo(general, read);

  b.say(
    jobs,
    marta,
    b.daysAgo(4, 9, 0),
    'We are hiring a Rust backend engineer (hybrid, Kreuzberg).',
  );
  b.readAll(jobs);
  b.say(
    showAndTell,
    paul,
    b.daysAgo(5, 21, 0),
    'Wrote a tiny TOTP crate this weekend: no_std and 300 lines.',
  );
  b.readAll(showAndTell);
  return server;
}

MockServer _homelab(DateTime now, MockIds ids) {
  const self = 3000, dan = 3001, ines = 3002, robo = 3003;
  const everyone = 30;
  final server = MockServer(
    key: 'homelab.local:7710',
    info: const ServerInfo(
      serverIdHex: '686f6d656c61622d3030303030303031',
      name: 'Homelab',
      ownerId: dan,
      everyoneRoleId: everyone,
    ),
    selfId: self,
    fingerprint: fakeFingerprint('homelab.local:7710'),
    unreachable: true,
  );
  final b = _Builder(server, now, ids, channelBase: 30000);
  b.role(everyone, '@everyone', 0, Permissions.defaultEveryone);
  b.person(self, selfName, joinedDaysAgo: 200);
  b.person(dan, 'Dan Moreau', presence: Presence.offline);
  b.person(ines, 'Inês Costa', presence: Presence.offline);
  b.person(robo, 'uptime-bot', presence: Presence.offline);
  final general = b.channel('general', ChannelKind.text);
  final monitoring = b.channel('monitoring', ChannelKind.text);
  b.channel('Garage', ChannelKind.voice);
  b.say(
    general,
    dan,
    b.daysAgo(2, 22, 0),
    'Router firmware updated, reboot at midnight.',
  );
  b.readAll(general);
  b.say(
    monitoring,
    robo,
    b.daysAgo(1, 3, 12),
    'nas-01 is down (no reply for 5 minutes)',
  );
  b.say(monitoring, robo, b.daysAgo(1, 3, 40), 'nas-01 is up again');
  b.readAll(monitoring);
  return server;
}
