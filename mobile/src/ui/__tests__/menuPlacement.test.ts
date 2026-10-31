import { MENU_MARGIN, placeMenu } from '../menuPlacement';

// A Pixel 7 in points, with its system bars.
const SCREEN = { width: 411, height: 914, top: 24, bottom: 48 };
const MENU = { width: 232, height: 156 };

describe('placeMenu', () => {
  it('opens below a button on the right, lined up with its right edge', () => {
    const button = { x: 367, y: 56, width: 44, height: 44 };
    expect(placeMenu(button, MENU, SCREEN)).toEqual({
      left: SCREEN.width - MENU_MARGIN - MENU.width,
      top: 56 + 44 + MENU_MARGIN,
      origin: 'top right',
    });
  });

  it("lines up with the user's own bubble on the right, above it when there's no room below", () => {
    const own = { x: 140, y: 700, width: 259, height: 60 };
    expect(placeMenu(own, MENU, SCREEN)).toEqual({
      left: 140 + 259 - MENU.width,
      top: 700 - MENU_MARGIN - MENU.height,
      origin: 'bottom right',
    });
  });

  it("lines up with someone else's bubble on the left", () => {
    const theirs = { x: 52, y: 300, width: 200, height: 60 };
    expect(placeMenu(theirs, MENU, SCREEN)).toMatchObject({ left: 52, origin: 'top left' });
  });

  it('starts a full-width row at the rows inset', () => {
    const row = { x: 0, y: 100, width: 411, height: 72 };
    expect(placeMenu(row, MENU, SCREEN)).toMatchObject({ left: 16, top: 100 + 72 + MENU_MARGIN });
  });

  it('stays on screen beside a bubble taller than the room above and below it', () => {
    const tall = { x: 60, y: 100, width: 280, height: 700 };
    const placed = placeMenu(tall, MENU, SCREEN);
    expect(placed.top).toBeGreaterThanOrEqual(SCREEN.top + MENU_MARGIN);
    expect(placed.top + MENU.height).toBeLessThanOrEqual(SCREEN.height - SCREEN.bottom - MENU_MARGIN);
  });
});
