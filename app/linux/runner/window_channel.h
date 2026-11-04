#ifndef RUNNER_WINDOW_CHANNEL_H_
#define RUNNER_WINDOW_CHANNEL_H_

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>

// Connects the "dev.opencord/window" method channel to `window`, so the app
// can draw its own title bar: drag, resize, the window menu, window state
// and geometry.
void window_channel_register(GtkWindow* window, FlView* view);

// Brings the window back, from the tray or from behind other windows, and
// passes `link` (an opencord:// link, or null) to the app. A later launch of
// the app lands here (desktop UI plan §15).
void window_channel_present(const gchar* link);

// Tells the compositor the app draws `window`'s frame itself. On Wayland an
// undecorated GTK window is not enough: GTK asks KWin for a server-side
// frame, so KDE showed its own title bar above the app's. Call it once
// the window is realized.
void window_channel_draw_own_frame(GtkWindow* window);

// Whether the app asked to start minimized to the tray, so the first frame
// should not show the window.
bool window_channel_starts_hidden();

#endif  // RUNNER_WINDOW_CHANNEL_H_
