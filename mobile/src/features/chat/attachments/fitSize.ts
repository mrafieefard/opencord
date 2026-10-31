/**
 * Computes thumbnail dimensions keeping the original aspect ratio within
 * bounds (spec §8.5: fitted within 260 × 320 pt).
 */
export function fitSize(
  width: number,
  height: number,
  maxW = 260,
  maxH = 320,
): { width: number; height: number } {
  if (width <= 0 || height <= 0 || !Number.isFinite(width) || !Number.isFinite(height)) {
    return { width: maxW, height: Math.round(maxW * 0.75) };
  }
  const scale = Math.min(1, maxW / width, maxH / height);
  return {
    width: Math.round(width * scale),
    height: Math.round(height * scale),
  };
}
