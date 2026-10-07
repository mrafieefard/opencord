import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/presence_state.dart';
import 'package:opencord/core/providers/server_state.dart';

/// Offline sections longer than this start collapsed (§4.7).
const offlineCollapseAt = 30;

/// A row of the member panel: a section header or a member.
sealed class MemberEntry {
  const MemberEntry();

  Object get key;
}

/// "MAINTAINER — 2".
final class MemberHeader extends MemberEntry {
  const MemberHeader(
    this.label,
    this.count, {
    this.collapsible = false,
    this.collapsed = false,
  });

  final String label;
  final int count;
  final bool collapsible;
  final bool collapsed;

  @override
  Object get key => 'section:$label';
}

final class MemberLine extends MemberEntry {
  const MemberLine(
    this.member, {
    required this.presence,
    this.activity,
    this.owner = false,
  });

  final Member member;
  final Presence presence;

  /// "In voice · General" or what they said they are doing.
  final String? activity;
  final bool owner;

  @override
  Object get key => member.id;
}

/// The member panel's sections (§4.7): one per hoisted role (highest
/// first) with the online members whose highest hoisted role it is, then
/// everyone else online, then everyone offline. [query] keeps the members
/// whose name contains it; empty sections are left out.
List<MemberEntry> memberSections(
  ServerData data,
  List<Member> members,
  PresenceState presence, {
  String query = '',
  String? Function(int userId)? voiceChannelOf,
  bool offlineExpanded = false,
}) {
  final needle = query.trim().toLowerCase();
  final shown =
      [
        for (final member in members)
          if (needle.isEmpty ||
              member.displayName.toLowerCase().contains(needle))
            member,
      ]..sort(
        (a, b) =>
            a.displayName.toLowerCase().compareTo(b.displayName.toLowerCase()),
      );

  MemberLine line(Member member) {
    final voice = voiceChannelOf?.call(member.id);
    return MemberLine(
      member,
      presence: presence.of(member.id),
      activity: voice != null
          ? 'In voice · $voice'
          : presence.activities[member.id],
      owner: data.info.ownerId == member.id,
    );
  }

  final online = shown
      .where((member) => presence.of(member.id) != Presence.offline)
      .toList();
  final offline = shown
      .where((member) => presence.of(member.id) == Presence.offline)
      .toList();
  final entries = <MemberEntry>[];
  final placed = <int>{};
  for (final role in data.rolesDescending.where((role) => role.hoist)) {
    final holders = [
      for (final member in online)
        if (data.highestHoistedRole(member)?.id == role.id) member,
    ];
    if (holders.isEmpty) continue;
    entries
      ..add(MemberHeader(role.name, holders.length))
      ..addAll(holders.map(line));
    placed.addAll(holders.map((member) => member.id));
  }
  final rest = online.where((member) => !placed.contains(member.id)).toList();
  if (rest.isNotEmpty) {
    entries
      ..add(MemberHeader('Online', rest.length))
      ..addAll(rest.map(line));
  }
  if (offline.isNotEmpty) {
    final collapsible = offline.length > offlineCollapseAt;
    final collapsed = collapsible && !offlineExpanded;
    entries.add(
      MemberHeader(
        'Offline',
        offline.length,
        collapsible: collapsible,
        collapsed: collapsed,
      ),
    );
    if (!collapsed) entries.addAll(offline.map(line));
  }
  return entries;
}
