#include "media_window.h"

#include <commctrl.h>
#include <dwmapi.h>
#include <flutter/standard_method_codec.h>
#include <windowsx.h>

#include <algorithm>
#include <string>
#include <utility>

#include "resource.h"

// From the engine's flutter_windows_internal.h, which the app's build does
// not ship: both are exported by flutter_windows.dll. They add a view to the
// running engine, as Flutter's own multi-window support does (D86).
extern "C" {
struct MediaViewProperties {
  int width;
  int height;
};
FLUTTER_EXPORT FlutterDesktopEngineRef FlutterDesktopEngineForId(
    int64_t engine_id);
FLUTTER_EXPORT FlutterDesktopViewControllerRef
FlutterDesktopEngineCreateViewController(
    FlutterDesktopEngineRef engine, const MediaViewProperties* properties);
}

namespace {

constexpr const wchar_t kWindowClass[] = L"OPENCORD_MEDIA_VIEWER";
constexpr UINT_PTR kViewSubclassId = 2;
// Aero Snap and Win+Up maximize: the viewer turns that into full screen,
// after the maximize has finished.
constexpr UINT kGoFullscreen = WM_APP + 0x52;
// Telegram's: the normal window is at least this big, and this big at first.
constexpr int kMinWidth = 480;
constexpr int kMinHeight = 360;
constexpr int kDefaultWidth = 800;
constexpr int kDefaultHeight = 600;
// Picture-in-picture (D87): this wide at first, at least this small, and
// this far in from the work area's bottom right corner, in logical pixels.
constexpr int kPipWidth = 320;
constexpr int kPipMinWidth = 160;
constexpr int kPipMargin = 24;
// Picture-in-picture's resize band at its edges, in logical pixels.
constexpr int kPipEdge = 6;

const flutter::EncodableValue* Find(const flutter::EncodableMap& map,
                                    const char* key) {
  auto entry = map.find(flutter::EncodableValue(key));
  return entry == map.end() ? nullptr : &entry->second;
}

std::optional<int64_t> IntArg(const flutter::EncodableMap& map,
                              const char* key) {
  const flutter::EncodableValue* value = Find(map, key);
  if (value == nullptr) return std::nullopt;
  if (const auto* number = std::get_if<int32_t>(value)) return *number;
  if (const auto* number = std::get_if<int64_t>(value)) return *number;
  return std::nullopt;
}

std::optional<double> DoubleArg(const flutter::EncodableMap& map,
                                const char* key) {
  const flutter::EncodableValue* value = Find(map, key);
  if (value == nullptr) return std::nullopt;
  if (const auto* number = std::get_if<double>(value)) return *number;
  return std::nullopt;
}

std::string StringArg(const flutter::EncodableMap& map, const char* key) {
  const flutter::EncodableValue* value = Find(map, key);
  const auto* text = value ? std::get_if<std::string>(value) : nullptr;
  return text ? *text : std::string();
}

// The desktop shows through where the viewer paints nothing: the accent
// policy flutter_acrylic uses for its transparent effect, with a fully
// transparent tint. The viewer paints its own dark background.
void MakeSeeThrough(HWND window) {
  struct AccentPolicy {
    int state;
    int flags;
    DWORD gradient;
    int animation;
  };
  struct CompositionData {
    int attribute;
    void* data;
    SIZE_T size;
  };
  using SetCompositionAttribute = BOOL(WINAPI*)(HWND, CompositionData*);
  HMODULE user32 = ::GetModuleHandleW(L"user32.dll");
  if (user32 == nullptr) return;
  auto set = reinterpret_cast<SetCompositionAttribute>(
      ::GetProcAddress(user32, "SetWindowCompositionAttribute"));
  if (set == nullptr) return;
  AccentPolicy accent = {2 /* ACCENT_ENABLE_TRANSPARENTGRADIENT */, 2, 0, 0};
  CompositionData data = {19 /* WCA_ACCENT_POLICY */, &accent, sizeof(accent)};
  set(window, &data);
  // One non-negative margin keeps the DWM shadow of the app-drawn frame.
  MARGINS margins = {0, 0, 1, 0};
  ::DwmExtendFrameIntoClientArea(window, &margins);
}

const wchar_t* RegisterClassOnce(WNDPROC proc) {
  static bool registered = false;
  if (!registered) {
    WNDCLASSW window_class = {};
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = proc;
    window_class.hInstance = ::GetModuleHandleW(nullptr);
    window_class.hIcon = ::LoadIconW(window_class.hInstance,
                                     MAKEINTRESOURCEW(IDI_APP_ICON));
    window_class.hCursor = ::LoadCursorW(nullptr, IDC_ARROW);
    window_class.lpszClassName = kWindowClass;
    registered = ::RegisterClassW(&window_class) != 0;
  }
  return kWindowClass;
}

RECT MonitorArea(HWND window, bool work) {
  MONITORINFO monitor = {sizeof(MONITORINFO)};
  ::GetMonitorInfoW(::MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
                    &monitor);
  return work ? monitor.rcWork : monitor.rcMonitor;
}

}  // namespace

MediaWindowChannel::MediaWindowChannel(HWND main,
                                       flutter::BinaryMessenger* messenger)
    : main_(main),
      channel_(std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          messenger, "dev.opencord/media_window",
          &flutter::StandardMethodCodec::GetInstance())) {
  channel_->SetMethodCallHandler([this](const auto& call, auto result) {
    HandleMethodCall(call, std::move(result));
  });
}

MediaWindowChannel::~MediaWindowChannel() {
  channel_->SetMethodCallHandler(nullptr);
  Close();
}

double MediaWindowChannel::Scale() const {
  return ::GetDpiForWindow(window_ != nullptr ? window_ : main_) / 96.0;
}

int MediaWindowChannel::FrameX() const {
  const UINT dpi = ::GetDpiForWindow(window_);
  return ::GetSystemMetricsForDpi(SM_CXFRAME, dpi) +
         ::GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
}

int MediaWindowChannel::FrameY() const {
  const UINT dpi = ::GetDpiForWindow(window_);
  return ::GetSystemMetricsForDpi(SM_CYFRAME, dpi) +
         ::GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
}

std::optional<int64_t> MediaWindowChannel::Open(
    const flutter::EncodableMap& args) {
  if (window_ != nullptr) {
    ::SetForegroundWindow(window_);
    return FlutterDesktopViewControllerGetViewId(controller_);
  }
  const std::optional<int64_t> engine_id = IntArg(args, "engineId");
  FlutterDesktopEngineRef engine =
      engine_id ? FlutterDesktopEngineForId(*engine_id) : nullptr;
  if (engine == nullptr) return std::nullopt;

  fullscreen_ = StringArg(args, "mode") != "normal";
  // The normal window: logical size, physical place (as the main window
  // keeps them); centred on the main window's monitor when the place is
  // gone or unknown.
  const double scale = ::GetDpiForWindow(main_) / 96.0;
  const int width = static_cast<int>(
      std::max<int64_t>(IntArg(args, "width").value_or(kDefaultWidth),
                        kMinWidth) *
      scale);
  const int height = static_cast<int>(
      std::max<int64_t>(IntArg(args, "height").value_or(kDefaultHeight),
                        kMinHeight) *
      scale);
  RECT rect = {0, 0, width, height};
  const std::optional<int64_t> x = IntArg(args, "x");
  const std::optional<int64_t> y = IntArg(args, "y");
  bool placed = false;
  if (x && y) {
    ::OffsetRect(&rect, static_cast<int>(*x), static_cast<int>(*y));
    placed = ::MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) != nullptr;
  }
  if (!placed) {
    const RECT area = MonitorArea(main_, true);
    rect.left = area.left + (area.right - area.left - width) / 2;
    rect.top = area.top + (area.bottom - area.top - height) / 2;
    rect.right = rect.left + width;
    rect.bottom = rect.top + height;
  }
  normal_ = rect;

  // Unowned, so it has a taskbar button of its own.
  window_ = ::CreateWindowExW(
      0, RegisterClassOnce(WndProc), L"Opencord media viewer",
      WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN, rect.left, rect.top, width,
      height, nullptr, nullptr, ::GetModuleHandleW(nullptr), this);
  if (window_ == nullptr) return std::nullopt;
  MakeSeeThrough(window_);

  RECT client;
  ::GetClientRect(window_, &client);
  const MediaViewProperties properties = {
      static_cast<int>(client.right - client.left),
      static_cast<int>(client.bottom - client.top)};
  controller_ = FlutterDesktopEngineCreateViewController(engine, &properties);
  if (controller_ == nullptr) {
    ::DestroyWindow(window_);
    window_ = nullptr;
    return std::nullopt;
  }
  view_ = FlutterDesktopViewGetHWND(
      FlutterDesktopViewControllerGetView(controller_));
  ::SetParent(view_, window_);
  ::SetWindowSubclass(view_, ViewProc, kViewSubclassId,
                      reinterpret_cast<DWORD_PTR>(this));
  // Recompute the frame now that WM_NCCALCSIZE is handled.
  ::SetWindowPos(window_, nullptr, 0, 0, 0, 0,
                 SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER |
                     SWP_NOACTIVATE);
  if (fullscreen_) {
    fullscreen_ = false;
    SetFullscreen(true);
  }
  LayoutView();
  ::ShowWindow(window_, SW_SHOW);
  ::SetForegroundWindow(window_);
  ::SetFocus(view_);
  return FlutterDesktopViewControllerGetViewId(controller_);
}

void MediaWindowChannel::Close() {
  if (window_ == nullptr) return;
  if (view_ != nullptr) {
    ::RemoveWindowSubclass(view_, ViewProc, kViewSubclassId);
  }
  // The view goes before its window: the engine stops drawing into it.
  if (controller_ != nullptr) {
    FlutterDesktopViewControllerDestroy(controller_);
    controller_ = nullptr;
  }
  view_ = nullptr;
  HWND window = window_;
  window_ = nullptr;
  ::DestroyWindow(window);
}

void MediaWindowChannel::SetFullscreen(bool fullscreen) {
  if (window_ == nullptr) return;
  // From picture-in-picture either way the style and place change, and its
  // small frame is never taken for the normal window's.
  const bool from_pip = pip_;
  if (from_pip) {
    LeavePip();
  } else if (fullscreen == fullscreen_) {
    return;
  }
  if (fullscreen) {
    if (!from_pip && !::IsIconic(window_) && !::IsZoomed(window_)) {
      ::GetWindowRect(window_, &normal_);
    }
    fullscreen_ = true;
    const DWORD style = WS_POPUP | WS_CLIPCHILDREN | WS_MINIMIZEBOX |
                        WS_SYSMENU |
                        (::IsWindowVisible(window_) ? WS_VISIBLE : 0u);
    ::SetWindowLongW(window_, GWL_STYLE, static_cast<LONG>(style));
    // The main window's monitor, taskbar included.
    const RECT area = MonitorArea(main_, false);
    ::SetWindowPos(window_, HWND_TOP, area.left, area.top,
                   area.right - area.left, area.bottom - area.top,
                   SWP_NOOWNERZORDER | SWP_FRAMECHANGED);
  } else {
    fullscreen_ = false;
    const DWORD style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN |
                        (::IsWindowVisible(window_) ? WS_VISIBLE : 0u);
    ::SetWindowLongW(window_, GWL_STYLE, static_cast<LONG>(style));
    ::SetWindowPos(window_, nullptr, normal_.left, normal_.top,
                   normal_.right - normal_.left, normal_.bottom - normal_.top,
                   SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED);
  }
  LayoutView();
  SendStatus();
}

void MediaWindowChannel::EnterPip(const flutter::EncodableMap& args) {
  if (window_ == nullptr || pip_) return;
  if (!fullscreen_ && !::IsIconic(window_) && !::IsZoomed(window_)) {
    ::GetWindowRect(window_, &normal_);
  }
  if (::IsZoomed(window_)) ::ShowWindow(window_, SW_RESTORE);
  const double aspect = DoubleArg(args, "aspect").value_or(16.0 / 9.0);
  pip_aspect_ = aspect > 0.1 && aspect < 10 ? aspect : 16.0 / 9.0;
  pip_ = true;
  fullscreen_ = false;

  const double scale = Scale();
  const int width = static_cast<int>(
      std::max<int64_t>(IntArg(args, "width").value_or(kPipWidth),
                        kPipMinWidth) *
      scale);
  const int height = static_cast<int>(width / pip_aspect_ + 0.5);
  RECT rect = {0, 0, width, height};
  const std::optional<int64_t> x = IntArg(args, "x");
  const std::optional<int64_t> y = IntArg(args, "y");
  bool placed = false;
  if (x && y) {
    ::OffsetRect(&rect, static_cast<int>(*x), static_cast<int>(*y));
    placed = ::MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) != nullptr;
  }
  if (!placed) {
    const RECT area = MonitorArea(main_, true);
    const int margin = static_cast<int>(kPipMargin * scale);
    rect = {area.right - margin - width, area.bottom - margin - height,
            area.right - margin, area.bottom - margin};
  }
  const DWORD style = WS_POPUP | WS_CLIPCHILDREN | WS_MINIMIZEBOX |
                      WS_SYSMENU | WS_THICKFRAME |
                      (::IsWindowVisible(window_) ? WS_VISIBLE : 0u);
  ::SetWindowLongW(window_, GWL_STYLE, static_cast<LONG>(style));
  ::SetWindowPos(window_, HWND_TOPMOST, rect.left, rect.top,
                 rect.right - rect.left, rect.bottom - rect.top,
                 SWP_NOOWNERZORDER | SWP_FRAMECHANGED);
  LayoutView();
  SendStatus();
}

void MediaWindowChannel::LeavePip() {
  if (window_ == nullptr || !pip_) return;
  pip_ = false;
  ::SetWindowPos(window_, HWND_NOTOPMOST, 0, 0, 0, 0,
                 SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
}

void MediaWindowChannel::KeepPipShape(WPARAM edge, RECT* rect) const {
  const int min_width = static_cast<int>(kPipMinWidth * Scale());
  int width = std::max<int>(rect->right - rect->left, min_width);
  int height = rect->bottom - rect->top;
  if (edge == WMSZ_TOP || edge == WMSZ_BOTTOM) {
    width = std::max(static_cast<int>(height * pip_aspect_ + 0.5), min_width);
  }
  height = static_cast<int>(width / pip_aspect_ + 0.5);
  // The corner or edges away from the one dragged stay put.
  if (edge == WMSZ_LEFT || edge == WMSZ_TOPLEFT || edge == WMSZ_BOTTOMLEFT) {
    rect->left = rect->right - width;
  } else {
    rect->right = rect->left + width;
  }
  if (edge == WMSZ_TOP || edge == WMSZ_TOPLEFT || edge == WMSZ_TOPRIGHT) {
    rect->top = rect->bottom - height;
  } else {
    rect->bottom = rect->top + height;
  }
}

void MediaWindowChannel::LayoutView() {
  if (window_ == nullptr || view_ == nullptr) return;
  RECT client;
  ::GetClientRect(window_, &client);
  ::MoveWindow(view_, 0, 0, client.right - client.left,
               client.bottom - client.top, TRUE);
}

LRESULT MediaWindowChannel::HitTest(POINT screen_point) const {
  if (fullscreen_ || ::IsZoomed(window_)) return HTCLIENT;
  POINT point = screen_point;
  ::ScreenToClient(window_, &point);
  if (pip_) {
    // No frame: its edges resize it.
    RECT client;
    ::GetClientRect(window_, &client);
    const int band = static_cast<int>(kPipEdge * Scale());
    const bool left = point.x < band;
    const bool right = point.x >= client.right - band;
    const bool top = point.y < band;
    const bool bottom = point.y >= client.bottom - band;
    if (top && left) return HTTOPLEFT;
    if (top && right) return HTTOPRIGHT;
    if (bottom && left) return HTBOTTOMLEFT;
    if (bottom && right) return HTBOTTOMRIGHT;
    if (left) return HTLEFT;
    if (right) return HTRIGHT;
    if (top) return HTTOP;
    if (bottom) return HTBOTTOM;
    return HTCLIENT;
  }
  if (point.y >= 0 && point.y < FrameY()) {
    RECT client;
    ::GetClientRect(window_, &client);
    if (point.x < 2 * FrameX()) return HTTOPLEFT;
    if (point.x >= client.right - 2 * FrameX()) return HTTOPRIGHT;
    return HTTOP;
  }
  return HTCLIENT;
}

void MediaWindowChannel::SendStatus() {
  if (window_ == nullptr) return;
  flutter::EncodableMap map;
  map[flutter::EncodableValue("mode")] = flutter::EncodableValue(std::string(
      pip_ ? "pip" : (fullscreen_ ? "fullscreen" : "normal")));
  map[flutter::EncodableValue("focused")] =
      flutter::EncodableValue(::GetForegroundWindow() == window_);
  channel_->InvokeMethod("status",
                         std::make_unique<flutter::EncodableValue>(map));
}

void MediaWindowChannel::SendGeometry() {
  if (window_ == nullptr || fullscreen_ || ::IsIconic(window_) ||
      ::IsZoomed(window_)) {
    return;
  }
  RECT rect;
  ::GetWindowRect(window_, &rect);
  // Picture-in-picture's frame is reported apart, and the normal window's
  // place stays.
  if (!pip_) normal_ = rect;
  const double scale = Scale();
  flutter::EncodableMap map;
  map[flutter::EncodableValue("width")] = flutter::EncodableValue(
      static_cast<int>((rect.right - rect.left) / scale));
  map[flutter::EncodableValue("height")] = flutter::EncodableValue(
      static_cast<int>((rect.bottom - rect.top) / scale));
  map[flutter::EncodableValue("x")] =
      flutter::EncodableValue(static_cast<int>(rect.left));
  map[flutter::EncodableValue("y")] =
      flutter::EncodableValue(static_cast<int>(rect.top));
  channel_->InvokeMethod("geometry",
                         std::make_unique<flutter::EncodableValue>(map));
}

void MediaWindowChannel::HandleMethodCall(
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  const std::string& method = call.method_name();
  const flutter::EncodableValue* args = call.arguments();
  if (method == "open") {
    const auto* map =
        args == nullptr ? nullptr : std::get_if<flutter::EncodableMap>(args);
    const std::optional<int64_t> view =
        map == nullptr ? std::nullopt : Open(*map);
    if (view) {
      result->Success(flutter::EncodableValue(*view));
    } else {
      result->Success();
    }
    return;
  }
  if (method == "close") {
    Close();
  } else if (method == "setMode") {
    const auto* mode =
        args == nullptr ? nullptr : std::get_if<std::string>(args);
    if (mode != nullptr) SetFullscreen(*mode == "fullscreen");
  } else if (method == "pip") {
    const auto* map =
        args == nullptr ? nullptr : std::get_if<flutter::EncodableMap>(args);
    if (map != nullptr) EnterPip(*map);
  } else if (method == "minimize") {
    if (window_ != nullptr) ::ShowWindow(window_, SW_MINIMIZE);
  } else if (method == "focus") {
    if (window_ != nullptr) {
      if (::IsIconic(window_)) ::ShowWindow(window_, SW_RESTORE);
      ::SetForegroundWindow(window_);
    }
  } else if (method == "startDrag") {
    if (window_ != nullptr && !fullscreen_) {
      ::ReleaseCapture();
      ::SendMessageW(window_, WM_SYSCOMMAND, SC_MOVE | HTCAPTION, 0);
    }
  } else if (method == "startResize") {
    // Windows resizes from the window's own borders.
  } else {
    result->NotImplemented();
    return;
  }
  result->Success();
}

LRESULT CALLBACK MediaWindowChannel::WndProc(HWND hwnd, UINT message,
                                             WPARAM wparam, LPARAM lparam) {
  if (message == WM_NCCREATE) {
    auto* create = reinterpret_cast<CREATESTRUCTW*>(lparam);
    ::SetWindowLongPtrW(hwnd, GWLP_USERDATA,
                        reinterpret_cast<LONG_PTR>(create->lpCreateParams));
  } else if (auto* self = reinterpret_cast<MediaWindowChannel*>(
                 ::GetWindowLongPtrW(hwnd, GWLP_USERDATA))) {
    if (self->window_ == hwnd) {
      return self->HandleMessage(hwnd, message, wparam, lparam);
    }
  }
  return ::DefWindowProcW(hwnd, message, wparam, lparam);
}

LRESULT CALLBACK MediaWindowChannel::ViewProc(HWND hwnd, UINT message,
                                              WPARAM wparam, LPARAM lparam,
                                              UINT_PTR, DWORD_PTR data) {
  if (message == WM_NCHITTEST) {
    auto* self = reinterpret_cast<MediaWindowChannel*>(data);
    const LRESULT hit =
        self->HitTest({GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
    // Let the window answer for its top resize strip.
    if (hit != HTCLIENT) return HTTRANSPARENT;
  }
  return ::DefSubclassProc(hwnd, message, wparam, lparam);
}

LRESULT MediaWindowChannel::HandleMessage(HWND hwnd, UINT message,
                                          WPARAM wparam, LPARAM lparam) {
  // Flutter and its plugins first.
  if (controller_ != nullptr) {
    LRESULT handled = 0;
    if (FlutterDesktopViewControllerHandleTopLevelWindowProc(
            controller_, hwnd, message, wparam, lparam, &handled)) {
      return handled;
    }
  }
  switch (message) {
    case WM_NCCALCSIZE: {
      if (wparam == FALSE) break;
      if (fullscreen_ || pip_) return 0;
      // Keep the side and bottom borders (they resize, and DWM draws the
      // shadow there); the client area takes the title bar's place.
      auto* params = reinterpret_cast<NCCALCSIZE_PARAMS*>(lparam);
      RECT& rect = params->rgrc[0];
      rect.left += FrameX();
      rect.right -= FrameX();
      rect.bottom -= FrameY();
      if (::IsZoomed(hwnd)) rect.top += FrameY();
      return 0;
    }
    case WM_NCHITTEST: {
      if (fullscreen_) return HTCLIENT;
      if (pip_) {
        return HitTest({GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
      }
      const LRESULT frame = ::DefWindowProcW(hwnd, message, wparam, lparam);
      if (frame != HTCLIENT) return frame;
      return HitTest({GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
    }
    case WM_GETMINMAXINFO: {
      auto* info = reinterpret_cast<MINMAXINFO*>(lparam);
      const double scale = Scale();
      if (pip_) {
        info->ptMinTrackSize.x = static_cast<LONG>(kPipMinWidth * scale);
        info->ptMinTrackSize.y =
            static_cast<LONG>(kPipMinWidth * scale / pip_aspect_);
        return 0;
      }
      info->ptMinTrackSize.x =
          static_cast<LONG>(kMinWidth * scale) + 2 * FrameX();
      info->ptMinTrackSize.y = static_cast<LONG>(kMinHeight * scale) + FrameY();
      return 0;
    }
    case WM_SIZE:
      LayoutView();
      if (wparam == SIZE_MAXIMIZED && !fullscreen_) {
        ::PostMessageW(hwnd, kGoFullscreen, 0, 0);
      } else {
        SendGeometry();
      }
      return 0;
    case WM_SIZING:
      if (pip_) {
        KeepPipShape(wparam, reinterpret_cast<RECT*>(lparam));
        return TRUE;
      }
      break;
    case kGoFullscreen:
      if (::IsZoomed(hwnd)) ::ShowWindow(hwnd, SW_RESTORE);
      SetFullscreen(true);
      return 0;
    case WM_MOVE:
      SendGeometry();
      return 0;
    case WM_ACTIVATE:
      if (LOWORD(wparam) != WA_INACTIVE && view_ != nullptr) {
        ::SetFocus(view_);
      }
      SendStatus();
      return 0;
    case WM_DPICHANGED: {
      if (fullscreen_) {
        const RECT area = MonitorArea(hwnd, false);
        ::SetWindowPos(hwnd, nullptr, area.left, area.top,
                       area.right - area.left, area.bottom - area.top,
                       SWP_NOZORDER | SWP_NOACTIVATE);
      } else {
        const auto* suggested = reinterpret_cast<const RECT*>(lparam);
        ::SetWindowPos(hwnd, nullptr, suggested->left, suggested->top,
                       suggested->right - suggested->left,
                       suggested->bottom - suggested->top,
                       SWP_NOZORDER | SWP_NOACTIVATE);
      }
      return 0;
    }
    case WM_ERASEBKGND:
      // The viewer paints everything, see-through where it wants.
      return 1;
    case WM_CLOSE:
      // Alt+F4 and the taskbar's Close close the viewer, never the app:
      // the app decides, and destroys the window.
      channel_->InvokeMethod("closeRequested", nullptr);
      return 0;
  }
  return ::DefWindowProcW(hwnd, message, wparam, lparam);
}
