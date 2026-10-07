#include "window_channel.h"

#include <cstring>
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

namespace {

constexpr int kTiled = GDK_WINDOW_STATE_TILED | GDK_WINDOW_STATE_TOP_TILED |
                       GDK_WINDOW_STATE_RIGHT_TILED |
                       GDK_WINDOW_STATE_BOTTOM_TILED |
                       GDK_WINDOW_STATE_LEFT_TILED;

struct WindowChannel {
  WindowChrome chrome = WindowChrome::kCustom;
  GtkWindow* window = nullptr;
  FlView* view = nullptr;
  FlMethodChannel* channel = nullptr;
  // The most recent button press: drag, resize and the window menu need it
  // (on Wayland the compositor checks it belongs to a real click).
  GdkEvent* last_press = nullptr;
  bool intercept_close = false;
  // Transparent space around the visible window for the custom frame's
  // shadow, and the part of it next to the window that resizes it.
  int frame_margin = 0;
  int resize_band = 0;
  GdkWindowState state = static_cast<GdkWindowState>(0);
};

// One window per process.
WindowChannel* channel_state = nullptr;

void remember_presses(GdkEvent* event, gpointer) {
  if (event->type == GDK_BUTTON_PRESS && channel_state != nullptr) {
    if (channel_state->last_press != nullptr) {
      gdk_event_free(channel_state->last_press);
    }
    channel_state->last_press = gdk_event_copy(event);
  }
  gtk_main_do_event(event);
}

bool is_wayland(GtkWindow* window) {
#ifdef GDK_WINDOWING_WAYLAND
  return GDK_IS_WAYLAND_DISPLAY(gtk_widget_get_display(GTK_WIDGET(window)));
#else
  (void)window;
  return false;
#endif
}

bool framed(const WindowChannel* self) {
  return (self->state &
          (GDK_WINDOW_STATE_MAXIMIZED | GDK_WINDOW_STATE_FULLSCREEN | kTiled)) ==
         0;
}

// Tells the compositor where the visible window is, and lets clicks on the
// outer, shadow-only part of the margin go to whatever is behind.
void update_frame(WindowChannel* self) {
  GdkWindow* gdk_window = gtk_widget_get_window(GTK_WIDGET(self->window));
  if (gdk_window == nullptr) return;
  const int margin = framed(self) ? self->frame_margin : 0;
  gdk_window_set_shadow_width(gdk_window, margin, margin, margin, margin);
  if (margin == 0) {
    gdk_window_input_shape_combine_region(gdk_window, nullptr, 0, 0);
    return;
  }
  const int inset = margin - self->resize_band;
  const int width = gdk_window_get_width(gdk_window);
  const int height = gdk_window_get_height(gdk_window);
  cairo_rectangle_int_t rect = {inset, inset, width - 2 * inset,
                                height - 2 * inset};
  cairo_region_t* region = cairo_region_create_rectangle(&rect);
  gdk_window_input_shape_combine_region(gdk_window, region, 0, 0);
  cairo_region_destroy(region);
}

FlValue* status_value(const WindowChannel* self) {
  FlValue* map = fl_value_new_map();
  fl_value_set_string_take(
      map, "maximized",
      fl_value_new_bool((self->state & GDK_WINDOW_STATE_MAXIMIZED) != 0));
  fl_value_set_string_take(
      map, "fullscreen",
      fl_value_new_bool((self->state & GDK_WINDOW_STATE_FULLSCREEN) != 0));
  fl_value_set_string_take(map, "tiled",
                           fl_value_new_bool((self->state & kTiled) != 0));
  fl_value_set_string_take(
      map, "focused",
      fl_value_new_bool((self->state & GDK_WINDOW_STATE_FOCUSED) != 0));
  return map;
}

gchar* decoration_layout() {
  gchar* layout = nullptr;
  g_object_get(gtk_settings_get_default(), "gtk-decoration-layout", &layout,
               nullptr);
  return layout;
}

void send(WindowChannel* self, const gchar* method, FlValue* args) {
  fl_method_channel_invoke_method(self->channel, method, args, nullptr,
                                  nullptr, nullptr);
}

gboolean on_state(GtkWidget*, GdkEventWindowState* event, gpointer data) {
  auto* self = static_cast<WindowChannel*>(data);
  self->state = event->new_window_state;
  update_frame(self);
  g_autoptr(FlValue) status = status_value(self);
  send(self, "status", status);
  return FALSE;
}

gboolean on_configure(GtkWidget*, GdkEventConfigure* event, gpointer data) {
  auto* self = static_cast<WindowChannel*>(data);
  update_frame(self);
  const int margin = framed(self) ? self->frame_margin : 0;
  g_autoptr(FlValue) geometry = fl_value_new_map();
  fl_value_set_string_take(geometry, "width",
                           fl_value_new_int(event->width - 2 * margin));
  fl_value_set_string_take(geometry, "height",
                           fl_value_new_int(event->height - 2 * margin));
  if (!is_wayland(self->window)) {
    gint x = 0, y = 0;
    gtk_window_get_position(self->window, &x, &y);
    fl_value_set_string_take(geometry, "x", fl_value_new_int(x + margin));
    fl_value_set_string_take(geometry, "y", fl_value_new_int(y + margin));
  }
  send(self, "geometry", geometry);
  return FALSE;
}

gboolean on_delete(GtkWidget*, GdkEvent*, gpointer data) {
  auto* self = static_cast<WindowChannel*>(data);
  if (!self->intercept_close) return FALSE;
  send(self, "closeRequested", nullptr);
  return TRUE;
}

void on_layout_changed(GObject*, GParamSpec*, gpointer data) {
  auto* self = static_cast<WindowChannel*>(data);
  g_autofree gchar* layout = decoration_layout();
  g_autoptr(FlValue) value = fl_value_new_string(layout ? layout : "");
  send(self, "buttonLayout", value);
}

int int_arg(FlValue* args, const char* key, int fallback) {
  FlValue* value = fl_value_lookup_string(args, key);
  return value != nullptr && fl_value_get_type(value) == FL_VALUE_TYPE_INT
             ? static_cast<int>(fl_value_get_int(value))
             : fallback;
}

bool bool_arg(FlValue* args, const char* key, bool fallback) {
  FlValue* value = fl_value_lookup_string(args, key);
  return value != nullptr && fl_value_get_type(value) == FL_VALUE_TYPE_BOOL
             ? fl_value_get_bool(value)
             : fallback;
}

bool visible_on_some_monitor(GdkDisplay* display, int x, int y, int width,
                             int height) {
  const int count = gdk_display_get_n_monitors(display);
  for (int i = 0; i < count; i++) {
    GdkRectangle area;
    gdk_monitor_get_workarea(gdk_display_get_monitor(display, i), &area);
    // At least a 100 px square of the title area must be on screen.
    if (x + 100 <= area.x + area.width && x + width - 100 >= area.x &&
        y >= area.y && y + 100 <= area.y + area.height) {
      return true;
    }
  }
  (void)height;
  return false;
}

const char* chrome_name(WindowChrome chrome) {
  switch (chrome) {
    case WindowChrome::kCustom:
      return "custom";
    case WindowChrome::kSystem:
      return "system";
    case WindowChrome::kBare:
      return "bare";
  }
  return "custom";
}

// Applies the app's startup settings. Sizes are of the visible window; on
// the custom frame the transparent margin is added around them.
FlValue* configure(WindowChannel* self, FlValue* args) {
  self->intercept_close = bool_arg(args, "interceptClose", false);

  GdkScreen* screen = gtk_window_get_screen(self->window);
  const bool transparent = gtk_widget_get_visual(GTK_WIDGET(self->window)) ==
                               gdk_screen_get_rgba_visual(screen) &&
                           gdk_screen_is_composited(screen);
  const bool frame = self->chrome == WindowChrome::kCustom && transparent;
  self->frame_margin = frame ? int_arg(args, "frameMargin", 0) : 0;
  self->resize_band = frame ? int_arg(args, "resizeBand", 0) : 0;
  const int margin = self->frame_margin;

  const int background = frame ? 0 : int_arg(args, "background", 0xFF000000);
  GdkRGBA color = {((background >> 16) & 0xFF) / 255.0,
                   ((background >> 8) & 0xFF) / 255.0,
                   (background & 0xFF) / 255.0,
                   ((background >> 24) & 0xFF) / 255.0};
  fl_view_set_background_color(self->view, &color);

  GdkGeometry hints = {};
  hints.min_width = int_arg(args, "minWidth", 940) + 2 * margin;
  hints.min_height = int_arg(args, "minHeight", 560) + 2 * margin;
  gtk_window_set_geometry_hints(self->window, nullptr, &hints,
                                GDK_HINT_MIN_SIZE);

  const int width = int_arg(args, "width", 1280);
  const int height = int_arg(args, "height", 800);
  gtk_window_set_default_size(self->window, width + 2 * margin,
                              height + 2 * margin);
  FlValue* x = fl_value_lookup_string(args, "x");
  FlValue* y = fl_value_lookup_string(args, "y");
  if (!is_wayland(self->window) && x != nullptr && y != nullptr &&
      fl_value_get_type(x) == FL_VALUE_TYPE_INT &&
      fl_value_get_type(y) == FL_VALUE_TYPE_INT) {
    const int left = static_cast<int>(fl_value_get_int(x));
    const int top = static_cast<int>(fl_value_get_int(y));
    if (visible_on_some_monitor(gtk_widget_get_display(GTK_WIDGET(self->window)),
                                left, top, width, height)) {
      gtk_window_move(self->window, left - margin, top - margin);
    } else {
      gtk_window_set_position(self->window, GTK_WIN_POS_CENTER);
    }
  } else {
    gtk_window_set_position(self->window, GTK_WIN_POS_CENTER);
  }
  if (bool_arg(args, "maximized", false)) gtk_window_maximize(self->window);
  update_frame(self);

  g_autofree gchar* layout = decoration_layout();
  FlValue* info = fl_value_new_map();
  fl_value_set_string_take(info, "chrome",
                           fl_value_new_string(chrome_name(self->chrome)));
  fl_value_set_string_take(info, "transparent", fl_value_new_bool(transparent));
  fl_value_set_string_take(info, "frameMargin", fl_value_new_int(margin));
  fl_value_set_string_take(info, "wayland",
                           fl_value_new_bool(is_wayland(self->window)));
  fl_value_set_string_take(info, "buttonLayout",
                           fl_value_new_string(layout ? layout : ""));
  fl_value_set_string_take(info, "status", status_value(self));
  return info;
}

GdkWindowEdge edge_from(const gchar* name) {
  struct Edge {
    const char* name;
    GdkWindowEdge edge;
  };
  static const Edge edges[] = {
      {"top", GDK_WINDOW_EDGE_NORTH},
      {"bottom", GDK_WINDOW_EDGE_SOUTH},
      {"left", GDK_WINDOW_EDGE_WEST},
      {"right", GDK_WINDOW_EDGE_EAST},
      {"topLeft", GDK_WINDOW_EDGE_NORTH_WEST},
      {"topRight", GDK_WINDOW_EDGE_NORTH_EAST},
      {"bottomLeft", GDK_WINDOW_EDGE_SOUTH_WEST},
      {"bottomRight", GDK_WINDOW_EDGE_SOUTH_EAST},
  };
  for (const Edge& candidate : edges) {
    if (name != nullptr && strcmp(name, candidate.name) == 0) {
      return candidate.edge;
    }
  }
  return GDK_WINDOW_EDGE_SOUTH_EAST;
}

void method_call_cb(FlMethodChannel*, FlMethodCall* call, gpointer data) {
  auto* self = static_cast<WindowChannel*>(data);
  const gchar* method = fl_method_call_get_name(call);
  FlValue* args = fl_method_call_get_args(call);
  const bool has_map =
      args != nullptr && fl_value_get_type(args) == FL_VALUE_TYPE_MAP;
  GdkEvent* press = self->last_press;
  g_autoptr(FlValue) result = nullptr;

  if (strcmp(method, "configure") == 0 && has_map) {
    result = configure(self, args);
  } else if (strcmp(method, "setTitle") == 0 &&
             fl_value_get_type(args) == FL_VALUE_TYPE_STRING) {
    gtk_window_set_title(self->window, fl_value_get_string(args));
  } else if (strcmp(method, "startDrag") == 0) {
    if (press != nullptr) {
      gtk_window_begin_move_drag(self->window,
                                 static_cast<gint>(press->button.button),
                                 static_cast<gint>(press->button.x_root),
                                 static_cast<gint>(press->button.y_root),
                                 press->button.time);
    }
  } else if (strcmp(method, "startResize") == 0 &&
             fl_value_get_type(args) == FL_VALUE_TYPE_STRING) {
    if (press != nullptr) {
      gtk_window_begin_resize_drag(self->window,
                                   edge_from(fl_value_get_string(args)),
                                   static_cast<gint>(press->button.button),
                                   static_cast<gint>(press->button.x_root),
                                   static_cast<gint>(press->button.y_root),
                                   press->button.time);
    }
  } else if (strcmp(method, "showWindowMenu") == 0) {
    GdkWindow* gdk_window = gtk_widget_get_window(GTK_WIDGET(self->window));
    const bool shown = gdk_window != nullptr && press != nullptr &&
                       gdk_window_show_window_menu(gdk_window, press);
    result = fl_value_new_bool(shown);
  } else if (strcmp(method, "minimize") == 0) {
    gtk_window_iconify(self->window);
  } else if (strcmp(method, "toggleMaximize") == 0) {
    if (self->state & GDK_WINDOW_STATE_MAXIMIZED) {
      gtk_window_unmaximize(self->window);
    } else {
      gtk_window_maximize(self->window);
    }
  } else if (strcmp(method, "setFullscreen") == 0 &&
             fl_value_get_type(args) == FL_VALUE_TYPE_BOOL) {
    if (fl_value_get_bool(args)) {
      gtk_window_fullscreen(self->window);
    } else {
      gtk_window_unfullscreen(self->window);
    }
  } else if (strcmp(method, "close") == 0) {
    gtk_window_close(self->window);
  } else if (strcmp(method, "quit") == 0) {
    self->intercept_close = false;
    gtk_window_close(self->window);
  } else if (strcmp(method, "hide") == 0) {
    gtk_widget_hide(GTK_WIDGET(self->window));
  } else if (strcmp(method, "show") == 0) {
    gtk_window_present(self->window);
  } else if (strcmp(method, "setUrgent") == 0 &&
             fl_value_get_type(args) == FL_VALUE_TYPE_BOOL) {
    gtk_window_set_urgency_hint(self->window, fl_value_get_bool(args));
  } else if (strcmp(method, "status") == 0) {
    result = status_value(self);
  } else {
    fl_method_call_respond_not_implemented(call, nullptr);
    return;
  }
  fl_method_call_respond_success(call, result, nullptr);
}

}  // namespace

WindowChrome window_chrome_at_startup() {
  g_autofree gchar* path = g_build_filename(g_get_user_data_dir(),
                                            APPLICATION_ID, "window-chrome",
                                            nullptr);
  g_autofree gchar* saved = nullptr;
  if (g_file_get_contents(path, &saved, nullptr, nullptr)) {
    g_strstrip(saved);
    if (strcmp(saved, "system") == 0) return WindowChrome::kSystem;
    if (strcmp(saved, "bare") == 0) return WindowChrome::kBare;
    if (strcmp(saved, "custom") == 0) return WindowChrome::kCustom;
  }
  static const char* const tiling[] = {"hyprland", "sway", "i3",  "river",
                                       "niri",     "bspwm", "dwm"};
  g_autofree gchar* desktop =
      g_ascii_strdown(g_getenv("XDG_CURRENT_DESKTOP") != nullptr
                          ? g_getenv("XDG_CURRENT_DESKTOP")
                          : "",
                      -1);
  g_auto(GStrv) names = g_strsplit(desktop, ":", -1);
  bool kde = false;
  for (gchar** name = names; *name != nullptr; name++) {
    g_strstrip(*name);
    for (const char* compositor : tiling) {
      if (strcmp(*name, compositor) == 0) return WindowChrome::kBare;
    }
    kde = kde || strcmp(*name, "kde") == 0;
  }
  return kde ? WindowChrome::kSystem : WindowChrome::kCustom;
}

void window_channel_register(GtkWindow* window, FlView* view,
                             WindowChrome chrome) {
  // Lives as long as the process, like the window it serves.
  static WindowChannel instance;
  WindowChannel* self = &instance;
  self->chrome = chrome;
  self->window = window;
  self->view = view;
  channel_state = self;

  FlEngine* engine = fl_view_get_engine(view);
  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  self->channel = fl_method_channel_new(fl_engine_get_binary_messenger(engine),
                                        "dev.opencord/window",
                                        FL_METHOD_CODEC(codec));
  fl_method_channel_set_method_call_handler(self->channel, method_call_cb,
                                            self, nullptr);
  gdk_event_handler_set(remember_presses, nullptr, nullptr);

  gtk_widget_add_events(GTK_WIDGET(window), GDK_STRUCTURE_MASK);
  g_signal_connect(window, "window-state-event", G_CALLBACK(on_state), self);
  g_signal_connect(window, "configure-event", G_CALLBACK(on_configure), self);
  g_signal_connect(window, "delete-event", G_CALLBACK(on_delete), self);
  g_signal_connect(gtk_settings_get_default(), "notify::gtk-decoration-layout",
                   G_CALLBACK(on_layout_changed), self);
}
