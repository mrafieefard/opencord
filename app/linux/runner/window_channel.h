#ifndef RUNNER_WINDOW_CHANNEL_H_
#define RUNNER_WINDOW_CHANNEL_H_

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>

// Who draws the window frame (desktop UI plan §3.1).
enum class WindowChrome { kCustom, kSystem, kBare };

// The chrome to create the window with. GTK decides about client-side
// decorations when the window is realized, before the app runs, so the app
// saves its choice in "window-chrome" in its data directory for the next
// start. Without that file (the first start) this applies the plan's Auto
// rules: KDE keeps its own frame, tiling compositors get none, everything
// else gets the custom one.
WindowChrome window_chrome_at_startup();

// Connects the "dev.opencord/window" method channel to `window`, so the app
// can draw its own title bar: drag, resize, the window menu, window state
// and geometry.
void window_channel_register(GtkWindow* window, FlView* view,
                             WindowChrome chrome);

// Brings the window back, from the tray or from behind other windows, and
// passes `link` (an opencord:// link, or null) to the app. A later launch of
// the app lands here (desktop UI plan §15).
void window_channel_present(const gchar* link);

#endif  // RUNNER_WINDOW_CHANNEL_H_
