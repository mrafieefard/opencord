#ifndef RUNNER_MEDIA_WINDOW_H_
#define RUNNER_MEDIA_WINDOW_H_

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>

// Connects the "dev.opencord/media_window" method channel (D86): the media
// viewer's own window, undecorated and see-through, with a second Flutter
// view on the engine of `main_view`. It opens full screen on the main
// window's monitor, or as a normal window of its remembered size.
void media_window_register(GtkWindow* main_window, FlView* main_view);

#endif  // RUNNER_MEDIA_WINDOW_H_
