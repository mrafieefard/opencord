import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/voice/waveform_painter.dart';

void main() {
  test('waveform_resampling_uses_max', () {
    final input = [0, 255, 0, 0];
    final resampled = resampleWaveform(input, 2);
    expect(resampled, equals([255, 0]));
  });
}
