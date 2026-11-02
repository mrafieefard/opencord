#include "media_window.h"

#include <cstring>
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

#include "window_channel.h"

namespace {

// Telegram's: the normal window is at least this big, and this big at first.
constexpr int kMinWidth = 480;
constexpr int kMinHeight = 360;
constexpr int kDefaultWidth = 800;
constexpr int kDefaultHeight = 600;
// Picture-in-picture (D87): this wide at first, at least this small, and
// this far in from the screen's bottom right corner.
constexpr int kPipWidth = 320;
constexpr int kPipMinWidth = 160;
constexpr int kPipMargin = 24;
// How long the compositor takes to answer new size limits.
constexpr guint kPipSettleMs = 100;

enum class Mode { kFullscreen, kNormal, kPip };

struct MediaWindow {
  GtkWindow* main = nullptr;
  FlView* main_view = nullptr;
  FlMethodChannel* channel = nullptr;
  // The viewer's window and view while it is open.
  GtkWindow* window = nullptr;
  FlView* view = nullptr;
  GdkWindowState state = static_cast<GdkWindowState>(0);
  Mode mode = Mode::kFullscreen;
  int normal_width = kDefaultWidth;
  int normal_height = kDefaultHeight;
  // Picture-in-picture's size, asked again while the compositor's own
  // configure still has the old one (KWin on Wayland), a few times.
  int pip_width = 0;
  int pip_height = 0;
  int pip_retries = 0;
};

bool is_wayland(GtkWindow* window) {
#ifdef GDK_WINDOWING_WAYLAND
  return GDK_IS_WAYLAND_DISPLAY(gtk_widget_get_display(GTK_WIDGET(window)));
#else
  (void)window;
  return false;
#endif
}

void send(MediaWindow* self, const gchar* method, FlValue* args) {
  fl_method_channel_invoke_method(self->channel, method, args, nullptr,
                                  nullptr, nullptr);
}

int int_arg(FlValue* args, const char* key, int fallback) {
  FlValue* value = fl_value_lookup_string(args, key);
  return value != nullptr && fl_value_get_type(value) == FL_VALUE_TYPE_INT
             ? static_cast<int>(fl_value_get_int(value))
             : fallback;
}

bool has_int(FlValue* args, const char* key) {
  FlValue* value = fl_value_lookup_string(args, key);
  return value != nullptr && fl_value_get_type(value) == FL_VALUE_TYPE_INT;
}

FlValue* status_value(const MediaWindow* self) {
  const char* mode = "normal";
  if (self->mode == Mode::kPip) {
    mode = "pip";
  } else if (self->state & GDK_WINDOW_STATE_FULLSCREEN) {
    mode = "fullscreen";
  } else if (self->state & GDK_WINDOW_STATE_MAXIMIZED) {
    mode = "maximized";
  }
  FlValue* map = fl_value_new_map();
  fl_value_set_string_take(map, "mode", fl_value_new_string(mode));
  fl_value_set_string_take(
      map, "focused",
      fl_value_new_bool((self->state & GDK_WINDOW_STATE_FOCUSED) != 0));
  return map;
}

// The main window's monitor, where the viewer opens full screen.
int main_monitor(MediaWindow* self) {
  GdkDisplay* display = gtk_widget_get_display(GTK_WIDGET(self->main));
  GdkWindow* gdk_window = gtk_widget_get_window(GTK_WIDGET(self->main));
  GdkMonitor* monitor =
      gdk_window != nullptr
          ? gdk_display_get_monitor_at_window(display, gdk_window)
          : gdk_display_get_primary_monitor(display);
  const int count = gdk_display_get_n_monitors(display);
  for (int i = 0; i < count; i++) {
    if (gdk_display_get_monitor(display, i) == monitor) return i;
  }
  return 0;
}

// Whether a point is on some monitor, so a remembered place is still there.
bool on_a_monitor(GdkDisplay* display, int x, int y) {
  const int count = gdk_display_get_n_monitors(display);
  for (int i = 0; i < count; i++) {
    GdkRectangle area;
    gdk_monitor_get_workarea(gdk_display_get_monitor(display, i), &area);
    if (x >= area.x && x + 100 <= area.x + area.width && y >= area.y &&
        y + 100 <= area.y + area.height) {
      return true;
    }
  }
  return false;
}

// The normal window's size limits, which picture-in-picture changes.
void set_normal_hints(GtkWindow* window) {
  GdkGeometry hints = {};
  hints.min_width = kMinWidth;
  hints.min_height = kMinHeight;
  gtk_window_set_geometry_hints(window, nullptr, &hints, GDK_HINT_MIN_SIZE);
}

void set_mode(MediaWindow* self, bool fullscreen) {
  const bool was_pip = self->mode == Mode::kPip;
  self->pip_retries = 0;
  self->mode = fullscreen ? Mode::kFullscreen : Mode::kNormal;
  if (self->window == nullptr) return;
  if (was_pip) {
    gtk_window_set_keep_above(self->window, FALSE);
    set_normal_hints(self->window);
  }
  if (fullscreen) {
    gtk_window_fullscreen_on_monitor(self->window,
                                     gtk_window_get_screen(self->main),
                                     main_monitor(self));
  } else {
    gtk_window_unfullscreen(self->window);
    gtk_window_resize(self->window, self->normal_width, self->normal_height);
  }
  // From picture-in-picture to a normal window no window state changes,
  // so nothing else would say so.
  if (was_pip) {
    g_autoptr(FlValue) status = status_value(self);
    send(self, "status", status);
  }
}

gboolean on_state(GtkWidget*, GdkEventWindowState* event, gpointer data) {
  auto* self = static_cast<MediaWindow*>(data);
  self->state = event->new_window_state;
  g_autoptr(FlValue) status = status_value(self);
  send(self, "status", status);
  return FALSE;
}

gboolean on_configure(GtkWidget*, GdkEventConfigure* event, gpointer data) {
  auto* self = static_cast<MediaWindow*>(data);
  if (self->mode == Mode::kFullscreen ||
      (self->state & (GDK_WINDOW_STATE_FULLSCREEN | GDK_WINDOW_STATE_MAXIMIZED |
                      GDK_WINDOW_STATE_TILED)) != 0) {
    return FALSE;
  }
  if (self->mode == Mode::kNormal) {
    self->normal_width = event->width;
    self->normal_height = event->height;
  } else if (self->pip_retries > 0) {
    if (event->width == self->pip_width) {
      self->pip_retries = 0;
    } else {
      self->pip_retries--;
      gtk_window_resize(self->window, self->pip_width, self->pip_height);
      return FALSE;
    }
  }
  g_autoptr(FlValue) geometry = fl_value_new_map();
  fl_value_set_string_take(geometry, "width", fl_value_new_int(event->width));
  fl_value_set_string_take(geometry, "height",
                           fl_value_new_int(event->height));
  if (!is_wayland(self->window)) {
    gint x = 0, y = 0;
    gtk_window_get_position(self->window, &x, &y);
    fl_value_set_string_take(geometry, "x", fl_value_new_int(x));
    fl_value_set_string_take(geometry, "y", fl_value_new_int(y));
  }
  send(self, "geometry", geometry);
  return FALSE;
}

// The system's close closes the viewer, never the app: the app decides.
// Connected before the view: the view's own handler asks the app to exit
// and ends the signal.
gboolean on_delete(GtkWidget*, GdkEvent*, gpointer data) {
  send(static_cast<MediaWindow*>(data), "closeRequested", nullptr);
  return TRUE;
}

void on_destroy(GtkWidget*, gpointer data) {
  auto* self = static_cast<MediaWindow*>(data);
  self->window = nullptr;
  self->view = nullptr;
  self->state = static_cast<GdkWindowState>(0);
}

FlValue* open(MediaWindow* self, FlValue* args) {
  if (self->window != nullptr) {
    gtk_window_present(self->window);
    return fl_value_new_int(fl_view_get_id(self->view));
  }
  FlValue* mode = fl_value_lookup_string(args, "mode");
  const bool fullscreen =
      mode == nullptr || fl_value_get_type(mode) != FL_VALUE_TYPE_STRING ||
      strcmp(fl_value_get_string(mode), "normal") != 0;
  self->normal_width =
      MAX(int_arg(args, "width", kDefaultWidth), kMinWidth);
  self->normal_height =
      MAX(int_arg(args, "height", kDefaultHeight), kMinHeight);

  GtkWindow* window = GTK_WINDOW(gtk_window_new(GTK_WINDOW_TOPLEVEL));
  self->window = window;
  // Before the view, whose own close handler would quit the app.
  g_signal_connect(window, "delete-event", G_CALLBACK(on_delete), self);
  g_signal_connect(window, "destroy", G_CALLBACK(on_destroy), self);
  GtkApplication* application = gtk_window_get_application(self->main);
  if (application != nullptr) gtk_window_set_application(window, application);
  gtk_window_set_title(window, "Opencord media viewer");
  gtk_window_set_decorated(window, FALSE);
  GdkScreen* screen = gtk_window_get_screen(window);
  GdkVisual* visual = gdk_screen_get_rgba_visual(screen);
  if (visual != nullptr && gdk_screen_is_composited(screen)) {
    gtk_widget_set_visual(GTK_WIDGET(window), visual);
    gtk_widget_set_app_paintable(GTK_WIDGET(window), TRUE);
  }
  set_normal_hints(window);
  gtk_window_set_default_size(window, self->normal_width, self->normal_height);
  if (!is_wayland(window) && has_int(args, "x") && has_int(args, "y") &&
      on_a_monitor(gtk_widget_get_display(GTK_WIDGET(window)),
                   int_arg(args, "x", 0), int_arg(args, "y", 0))) {
    gtk_window_move(window, int_arg(args, "x", 0), int_arg(args, "y", 0));
  } else {
    gtk_window_set_position(window, GTK_WIN_POS_CENTER);
  }

  FlView* view = fl_view_new_for_engine(fl_view_get_engine(self->main_view));
  self->view = view;
  GdkRGBA clear = {0, 0, 0, 0};
  fl_view_set_background_color(view, &clear);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));

  gtk_widget_add_events(GTK_WIDGET(window), GDK_STRUCTURE_MASK);
  g_signal_connect(window, "window-state-event", G_CALLBACK(on_state), self);
  g_signal_connect(window, "configure-event", G_CALLBACK(on_configure), self);

  // Undecorated is not enough on Wayland: KWin would add its own frame.
  gtk_widget_realize(GTK_WIDGET(window));
  window_channel_draw_own_frame(window);

  self->mode = fullscreen ? Mode::kFullscreen : Mode::kNormal;
  if (fullscreen) {
    gtk_window_fullscreen_on_monitor(window, gtk_window_get_screen(self->main),
                                     main_monitor(self));
  }
  gtk_widget_show(GTK_WIDGET(window));
  gtk_window_present(window);
  return fl_value_new_int(fl_view_get_id(view));
}

gboolean resize_pip(gpointer data) {
  auto* self = static_cast<MediaWindow*>(data);
  if (self->window != nullptr && self->mode == Mode::kPip) {
    gtk_window_resize(self->window, self->pip_width, self->pip_height);
  }
  return G_SOURCE_REMOVE;
}

// Picture-in-picture: the window small, with the video's shape, over other
// windows where the desktop lets it (not on Wayland, where apps can't), at
// the bottom right of the main window's screen or where it was.
void enter_pip(MediaWindow* self, FlValue* args) {
  if (self->window == nullptr) return;
  FlValue* aspect_value = fl_value_lookup_string(args, "aspect");
  double aspect = aspect_value != nullptr &&
                          fl_value_get_type(aspect_value) == FL_VALUE_TYPE_FLOAT
                      ? fl_value_get_float(aspect_value)
                      : 16.0 / 9.0;
  if (!(aspect > 0.1 && aspect < 10)) aspect = 16.0 / 9.0;
  const int width = MAX(int_arg(args, "width", kPipWidth), kPipMinWidth);
  const int height = static_cast<int>(width / aspect + 0.5);

  self->mode = Mode::kPip;
  GtkWindow* window = self->window;
  gtk_window_unfullscreen(window);
  GdkGeometry hints = {};
  hints.min_width = kPipMinWidth;
  hints.min_height = static_cast<int>(kPipMinWidth / aspect + 0.5);
  hints.min_aspect = aspect;
  hints.max_aspect = aspect;
  gtk_window_set_geometry_hints(
      window, nullptr, &hints,
      static_cast<GdkWindowHints>(GDK_HINT_MIN_SIZE | GDK_HINT_ASPECT));
  self->pip_width = width;
  self->pip_height = height;
  self->pip_retries = 2;
  gtk_window_resize(window, width, height);
  // The new limits make KWin (Wayland) answer with the old size, which
  // GTK takes over the one asked: ask again once that has come.
  g_timeout_add(kPipSettleMs, resize_pip, self);
  gtk_window_set_keep_above(window, TRUE);

  GdkDisplay* display = gtk_widget_get_display(GTK_WIDGET(window));
  if (has_int(args, "x") && has_int(args, "y") &&
      on_a_monitor(display, int_arg(args, "x", 0), int_arg(args, "y", 0))) {
    gtk_window_move(window, int_arg(args, "x", 0), int_arg(args, "y", 0));
  } else {
    GdkRectangle area;
    gdk_monitor_get_workarea(
        gdk_display_get_monitor(display, main_monitor(self)), &area);
    gtk_window_move(window, area.x + area.width - width - kPipMargin,
                    area.y + area.height - height - kPipMargin);
  }
  g_autoptr(FlValue) status = status_value(self);
  send(self, "status", status);
}

void method_call_cb(FlMethodChannel*, FlMethodCall* call, gpointer data) {
  auto* self = static_cast<MediaWindow*>(data);
  const gchar* method = fl_method_call_get_name(call);
  FlValue* args = fl_method_call_get_args(call);
  GdkEvent* press = window_channel_last_press();
  g_autoptr(FlValue) result = nullptr;

  if (strcmp(method, "open") == 0 && args != nullptr &&
      fl_value_get_type(args) == FL_VALUE_TYPE_MAP) {
    result = open(self, args);
  } else if (strcmp(method, "close") == 0) {
    if (self->window != nullptr) gtk_widget_destroy(GTK_WIDGET(self->window));
  } else if (strcmp(method, "setMode") == 0 && args != nullptr &&
             fl_value_get_type(args) == FL_VALUE_TYPE_STRING) {
    set_mode(self, strcmp(fl_value_get_string(args), "fullscreen") == 0);
  } else if (strcmp(method, "pip") == 0 && args != nullptr &&
             fl_value_get_type(args) == FL_VALUE_TYPE_MAP) {
    enter_pip(self, args);
  } else if (strcmp(method, "minimize") == 0) {
    if (self->window != nullptr) gtk_window_iconify(self->window);
  } else if (strcmp(method, "focus") == 0) {
    if (self->window != nullptr) gtk_window_present(self->window);
  } else if (strcmp(method, "startDrag") == 0) {
    if (self->window != nullptr && press != nullptr) {
      gtk_window_begin_move_drag(self->window,
                                 static_cast<gint>(press->button.button),
                                 static_cast<gint>(press->button.x_root),
                                 static_cast<gint>(press->button.y_root),
                                 press->button.time);
    }
  } else if (strcmp(method, "startResize") == 0 && args != nullptr &&
             fl_value_get_type(args) == FL_VALUE_TYPE_STRING) {
    if (self->window != nullptr && press != nullptr) {
      gtk_window_begin_resize_drag(
          self->window, window_channel_edge(fl_value_get_string(args)),
          static_cast<gint>(press->button.button),
          static_cast<gint>(press->button.x_root),
          static_cast<gint>(press->button.y_root), press->button.time);
    }
  } else {
    fl_method_call_respond_not_implemented(call, nullptr);
    return;
  }
  fl_method_call_respond_success(call, result, nullptr);
}

}  // namespace

void media_window_register(GtkWindow* main_window, FlView* main_view) {
  // Lives as long as the process, like the main window.
  static MediaWindow instance;
  MediaWindow* self = &instance;
  self->main = main_window;
  self->main_view = main_view;
  FlEngine* engine = fl_view_get_engine(main_view);
  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  self->channel = fl_method_channel_new(fl_engine_get_binary_messenger(engine),
                                        "dev.opencord/media_window",
                                        FL_METHOD_CODEC(codec));
  fl_method_channel_set_method_call_handler(self->channel, method_call_cb,
                                            self, nullptr);
}
