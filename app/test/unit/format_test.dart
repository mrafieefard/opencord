import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/format.dart';

final now = DateTime(2026, 10, 7, 14, 30); // a Wednesday

void main() {
  group('row times (§4.2)', () {
    test('today shows the time', () {
      expect(rowTime(DateTime(2026, 10, 7, 9, 5), now), '09:05');
    });

    test('this week shows the weekday', () {
      expect(rowTime(DateTime(2026, 10, 6, 23, 0), now), 'Tue');
      expect(rowTime(DateTime(2026, 10, 1, 8, 0), now), 'Thu');
    });

    test('older shows the date', () {
      expect(rowTime(DateTime(2026, 9, 30, 8, 0), now), '30.09.26');
      expect(rowTime(DateTime(2025, 12, 24, 8, 0), now), '24.12.25');
    });
  });

  group('day separators (§4.5)', () {
    test('name the recent days', () {
      expect(dayLabel(DateTime(2026, 10, 7, 1), now), 'Today');
      expect(dayLabel(DateTime(2026, 10, 6, 23), now), 'Yesterday');
      expect(dayLabel(DateTime(2026, 10, 2, 12), now), 'Friday');
    });

    test('older days show the date, with the year when it differs', () {
      expect(dayLabel(DateTime(2026, 9, 30), now), '30 September');
      expect(dayLabel(DateTime(2025, 10, 5), now), '5 October 2025');
    });
  });

  test('full timestamps for tooltips', () {
    expect(
      fullTimestamp(DateTime(2026, 10, 7, 9, 5)),
      'Wednesday, 7 October 2026 at 09:05',
    );
  });

  test('counts get thousands separators', () {
    expect(countLabel(7), '7');
    expect(countLabel(1204), '1,204');
    expect(countLabel(1234567), '1,234,567');
  });

  test('certificate fingerprints are grouped by four', () {
    expect(groupedFingerprint('9f3c2a0be1d4'), '9F3C 2A0B E1D4');
  });

  group('typing (§4.3)', () {
    test('names one or two people, then says several', () {
      expect(typingLabel(['Kai']), 'Kai is typing');
      expect(typingLabel(['Kai', 'Mira']), 'Kai and Mira are typing');
      expect(typingLabel(['Kai', 'Mira', 'Jo']), 'Several people are typing');
      expect(typingLabel([]), '');
    });
  });

  group('message previews', () {
    String? user(int id) => {1000: 'Alex', 1001: 'Kai'}[id];
    String? channel(int id) => {10: 'general'}[id];

    test('drop markdown and show mentions by name', () {
      expect(
        previewText(
          '**Design** at `15:00`, ping <@1000> in <#10>',
          user: user,
          channel: channel,
        ),
        'Design at 15:00, ping @Alex in #general',
      );
      expect(
        previewText('~~old~~ _new_ *plan*', user: user, channel: channel),
        'old new plan',
      );
    });

    test('code blocks and quotes collapse to their text on one line', () {
      expect(
        previewText(
          "Look:\n```rust\nlet x = 1;\nlet y = 2;\n```",
          user: user,
          channel: channel,
        ),
        'Look: let x = 1; let y = 2;',
      );
      expect(
        previewText('> quoted\nanswer', user: user, channel: channel),
        'quoted answer',
      );
    });

    test('unknown mentions stay readable', () {
      expect(
        previewText('hi <@42>', user: user, channel: channel),
        'hi @unknown',
      );
    });

    test('ids too big for 64 bits are not mentions and stay as typed', () {
      const typed = 'hi <@99999999999999999999> in <#99999999999999999999>';
      expect(previewText(typed, user: user, channel: channel), typed);
    });
  });

  test('long dates spell out the month', () {
    expect(longDate(DateTime(2026, 10, 5)), '5 October 2026');
  });

  test('expiry reads as a distance in time', () {
    final now = DateTime(2026, 10, 7, 12);
    expect(expiryLabel(null, now), 'Never');
    expect(
      expiryLabel(now.subtract(const Duration(minutes: 1)), now),
      'Expired',
    );
    expect(
      expiryLabel(now.add(const Duration(minutes: 30)), now),
      'in 30 minutes',
    );
    expect(expiryLabel(now.add(const Duration(hours: 1)), now), 'in 1 hour');
    expect(expiryLabel(now.add(const Duration(hours: 5)), now), 'in 5 hours');
    expect(
      expiryLabel(now.add(const Duration(days: 6, hours: 2)), now),
      'in 6 days',
    );
    expect(expiryLabel(now.add(const Duration(days: 1)), now), 'in 1 day');
  });
}
