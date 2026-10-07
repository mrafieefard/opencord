/// Text formats the UI shares (§4.2, §4.3, §4.5): times, days, counts,
/// fingerprints, typing and message previews. English for now.
library;

const _weekdays = [
  'Monday',
  'Tuesday',
  'Wednesday',
  'Thursday',
  'Friday',
  'Saturday',
  'Sunday',
];

const _months = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

String _two(int value) => value.toString().padLeft(2, '0');

DateTime _day(DateTime time) => DateTime(time.year, time.month, time.day);

int _daysBetween(DateTime earlier, DateTime later) =>
    _day(later).difference(_day(earlier)).inHours ~/ 24;

/// `HH:mm`.
String clockTime(DateTime time) => '${_two(time.hour)}:${_two(time.minute)}';

/// A channel row's time (§4.2): `HH:mm` today, the weekday this week,
/// `dd.MM.yy` before that.
String rowTime(DateTime time, DateTime now) {
  final days = _daysBetween(time, now);
  if (days <= 0) return clockTime(time);
  if (days < 7) return _weekdays[time.weekday - 1].substring(0, 3);
  return '${_two(time.day)}.${_two(time.month)}.${_two(time.year % 100)}';
}

/// A date separator (§4.5): Today, Yesterday, the weekday within the past
/// week, then `5 October` (with the year when it is not this year).
String dayLabel(DateTime day, DateTime now) {
  final days = _daysBetween(day, now);
  if (days <= 0) return 'Today';
  if (days == 1) return 'Yesterday';
  if (days < 7) return _weekdays[day.weekday - 1];
  final date = '${day.day} ${_months[day.month - 1]}';
  return day.year == now.year ? date : '$date ${day.year}';
}

/// `7 October 2026`, for "member since" (§4.8).
String longDate(DateTime time) =>
    '${time.day} ${_months[time.month - 1]} ${time.year}';

/// `Wednesday, 7 October 2026 at 09:05`, for timestamp tooltips (§16).
String fullTimestamp(DateTime time) =>
    '${_weekdays[time.weekday - 1]}, ${time.day} ${_months[time.month - 1]} '
    '${time.year} at ${clockTime(time)}';

/// `1,204`.
String countLabel(int count) {
  final digits = count.abs().toString();
  final buffer = StringBuffer(count < 0 ? '-' : '');
  for (var i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 == 0) buffer.write(',');
    buffer.write(digits[i]);
  }
  return buffer.toString();
}

/// A hex fingerprint in groups of four, upper case, for comparing by eye
/// (§4.11).
String groupedFingerprint(String hex) {
  final clean = hex.replaceAll(RegExp(r'[^0-9a-fA-F]'), '').toUpperCase();
  return [
    for (var i = 0; i < clean.length; i += 4)
      clean.substring(i, i + 4 > clean.length ? clean.length : i + 4),
  ].join(' ');
}

/// The chat header's typing line (§4.3).
String typingLabel(List<String> names) => switch (names.length) {
  0 => '',
  1 => '${names.first} is typing',
  2 => '${names.first} and ${names.last} are typing',
  _ => 'Several people are typing',
};

final _codeBlock = RegExp(r'```[^\n]*\n?([\s\S]*?)```');
final _inlineCode = RegExp('`([^`]*)`');
final _emphasis = RegExp(r'(\*\*|__|~~|\*|_)(\S(?:[\s\S]*?\S)?)\1');
final _userMention = RegExp(r'<@(\d+)>');
final _channelMention = RegExp(r'<#(\d+)>');
final _quote = RegExp(r'^>\s?', multiLine: true);
final _space = RegExp(r'\s+');

/// A message as one line of plain text, for channel-row previews (§4.2)
/// and reply quotes: markdown removed, mentions shown by name.
String previewText(
  String content, {
  required String? Function(int id) user,
  required String? Function(int id) channel,
}) {
  var text = content.replaceAllMapped(_codeBlock, (m) => ' ${m[1]} ');
  text = text.replaceAllMapped(_inlineCode, (m) => m[1]!);
  for (var previous = ''; previous != text;) {
    previous = text;
    text = text.replaceAllMapped(_emphasis, (m) => m[2]!);
  }
  text = text
      .replaceAllMapped(
        _userMention,
        (m) => '@${user(int.parse(m[1]!)) ?? 'unknown'}',
      )
      .replaceAllMapped(
        _channelMention,
        (m) => '#${channel(int.parse(m[1]!)) ?? 'unknown'}',
      )
      .replaceAll(_quote, '');
  return text.replaceAll(_space, ' ').trim();
}
