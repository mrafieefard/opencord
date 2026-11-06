/// What a member's presence shape shows (§5.3).
enum Presence {
  online('Online'),
  idle('Idle'),
  doNotDisturb('Do not disturb'),
  offline('Offline');

  const Presence(this.label);

  final String label;
}
