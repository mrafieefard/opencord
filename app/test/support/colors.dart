import 'dart:math' as math;
import 'dart:ui';

/// Largest gap between the red, green and blue channels (0–255) that still
/// counts as "no hue". The §2.1 greys carry a slight cool tint of up to 8
/// (for example `#6E6E76`); anything with real color is far above that.
const int maxChannelSpread = 8;

int channel(double value) => (value * 255).round();

int channelSpread(Color color) {
  final channels = [channel(color.r), channel(color.g), channel(color.b)];
  return channels.reduce(math.max) - channels.reduce(math.min);
}

bool isNeutral(Color color) => channelSpread(color) <= maxChannelSpread;

/// WCAG 2 contrast ratio of two opaque colors.
double contrastRatio(Color a, Color b) {
  final la = a.computeLuminance();
  final lb = b.computeLuminance();
  return (math.max(la, lb) + 0.05) / (math.min(la, lb) + 0.05);
}
