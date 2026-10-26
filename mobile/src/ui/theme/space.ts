// Space, shape and motion (design §9.3).
export const space = { xs: 4, sm: 8, md: 12, lg: 16, xl: 20, xxl: 24, xxxl: 32 } as const;

export const radius = { pill: 6, row: 8, image: 12, icon: 14, tile: 16, bubble: 18, sheet: 20 } as const;

export const motion = { press: 80, badge: 120, message: 120, speaking: 120, fade: 300 } as const;

/** Rows and controls at least this tall (accessibility). */
export const TOUCH = 44;
