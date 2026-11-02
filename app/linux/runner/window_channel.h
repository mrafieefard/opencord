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

// Watches the window's close button. Call it before the Flutter view is
// realized: the view then connects its own handler, which asks the app
// whether to exit and ends the signal, so a handler connected later never
// runs and closing quits instead of hiding to the tray.
void window_channel_watch_close(GtkWindow* window);

// The latest button press in any of the app's windows: moving and resizing
// need it (on Wayland the compositor checks it belongs to a real click).
// Null before the first press.
GdkEvent* window_channel_last_press();

// The GDK edge for a resize edge's name from the app ("top", "bottomLeft").
GdkWindowEdge window_channel_edge(const gchar* name);

// Whether the app asked to start minimized to the tray, so the first frame
// should not show the window.
bool window_channel_starts_hidden();

#endif  // RUNNER_WINDOW_CHANNEL_H_
