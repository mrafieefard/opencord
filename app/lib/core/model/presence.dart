/// What a member's presence shape shows (§5.3). "Invisible" is a choice
/// the user makes for themselves; everyone else sees [offline].
enum Presence {
  online('Online'),
  idle('Idle'),
  doNotDisturb('Do not disturb'),
  offline('Offline');

  const Presence(this.label);

  final String label;
}
