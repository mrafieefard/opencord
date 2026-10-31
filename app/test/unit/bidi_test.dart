import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/bidi.dart';

void main() {
  test('the first letter sets the direction', () {
    expect(textDirectionOf('سلام'), TextDirection.rtl);
    expect(textDirectionOf('שלום'), TextDirection.rtl);
    expect(textDirectionOf('hello سلام'), TextDirection.ltr);
    expect(textDirectionOf('سلام hello'), TextDirection.rtl);
    expect(textDirectionOf('Привет'), TextDirection.ltr);
  });

  test('digits, spaces and symbols before it do not count', () {
    expect(textDirectionOf('  12, 😀 سلام'), TextDirection.rtl);
    expect(textDirectionOf('@ #1 hi'), TextDirection.ltr);
  });

  test('no letter keeps the ambient direction', () {
    expect(textDirectionOf(''), isNull);
    expect(textDirectionOf('123 !?'), isNull);
    expect(textDirectionOf('😀'), isNull);
  });
}
