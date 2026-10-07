import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/presence_state.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/members/member_sections.dart';

import '../../support/fixtures.dart';

const lenaId = 1003;
const omarId = 1004;

void main() {
  // Maintainer (role 2) is hoisted; Alex owns the server.
  final data = ServerData.fromSnapshot(snapshot()).copyWith(
    members: {
      selfId: member(selfId, 'Alex'),
      kaiId: member(kaiId, 'Kai', roles: [2]),
      miraId: member(miraId, 'Mira'),
      lenaId: member(lenaId, 'Lena', roles: [2]),
      omarId: member(omarId, 'Omar'),
    },
  );
  final everyone = data.members.values.toList();
  const presence = PresenceState(
    presences: {
      selfId: Presence.online,
      kaiId: Presence.idle,
      miraId: Presence.doNotDisturb,
      lenaId: Presence.offline,
    },
    activities: {miraId: 'Editing main.rs'},
  );

  List<String> describe(List<MemberEntry> entries) => [
    for (final entry in entries)
      switch (entry) {
        MemberHeader(:final label, :final count) => '$label — $count',
        MemberLine(:final member) => member.displayName,
      },
  ];

  test('hoisted roles first, then online, then offline', () {
    expect(describe(memberSections(data, everyone, presence)), [
      'Maintainer — 1',
      'Kai',
      'Online — 2',
      'Alex',
      'Mira',
      'Offline — 2',
      'Lena',
      'Omar',
    ]);
  });

  test('lines carry presence, the owner star and activity', () {
    final lines = memberSections(
      data,
      everyone,
      presence,
    ).whereType<MemberLine>().toList();
    final alex = lines.firstWhere((line) => line.member.id == selfId);
    final mira = lines.firstWhere((line) => line.member.id == miraId);

    expect(alex.owner, isTrue);
    expect(mira.presence, Presence.doNotDisturb);
    expect(mira.activity, 'Editing main.rs');
  });

  test('voice beats activity text', () {
    final lines = memberSections(
      data,
      everyone,
      presence,
      voiceChannelOf: (id) => id == miraId ? 'General' : null,
    ).whereType<MemberLine>();

    expect(
      lines.firstWhere((line) => line.member.id == miraId).activity,
      'In voice · General',
    );
  });

  test('search keeps matching members and their sections', () {
    expect(describe(memberSections(data, everyone, presence, query: 'en')), [
      'Offline — 1',
      'Lena',
    ]);
  });

  test('a large offline section starts collapsed', () {
    final crowd = [
      ...everyone,
      for (var i = 0; i < offlineCollapseAt; i++)
        Member(
          user: User(id: 5000 + i, displayName: 'Guest $i'),
          joinedAt: t0,
        ),
    ];
    final entries = memberSections(data, crowd, presence);
    final header = entries.whereType<MemberHeader>().last;

    expect(header.label, 'Offline');
    expect(header.collapsed, isTrue);
    expect(entries.last, same(header));
    expect(
      memberSections(data, crowd, presence, offlineExpanded: true).last,
      isA<MemberLine>(),
    );
  });
}
