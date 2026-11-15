import { keyboardOverlap } from '../keyboardOverlap';

describe('how much the keyboard covers the area', () => {
  // Fails if the area's place on screen isn't counted (an area under a
  // header was lifted too little: the plugin's sheet stayed under the
  // keyboard), or a keyboard below the area lifts it.
  it('is the part of the area below the keyboard top', () => {
    // A 2400-pt screen, a 883-pt keyboard: its top is at 1517.
    expect(keyboardOverlap({ areaBottom: 2063, screenHeight: 2400, keyboardHeight: 883 })).toBe(546);
    expect(keyboardOverlap({ areaBottom: 1400, screenHeight: 2400, keyboardHeight: 883 })).toBe(0);
    expect(keyboardOverlap({ areaBottom: 2063, screenHeight: 2400, keyboardHeight: 0 })).toBe(0);
  });
});
