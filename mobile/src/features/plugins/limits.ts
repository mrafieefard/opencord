// Plugin areas' limits (plugins M4 plan, "Limits").

/** Plugin areas alive at once on one device: one plugin channel, one activity. */
export const AREAS_MAX = 2;

/** Batches pulled from the core per call. */
export const VIEW_BATCH_MAX = 64;

/** The least time between two `call` inputs to a view. */
export const CALL_INPUT_EVERY_MS = 100;
