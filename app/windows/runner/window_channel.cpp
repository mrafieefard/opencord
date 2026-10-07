#include "window_channel.h"

#include <commctrl.h>
#include <dwmapi.h>
#include <flutter/standard_method_codec.h>
#include <windowsx.h>

#include <string>

namespace {

// Caption buttons are 46 x 32 logical pixels at the top right (§3.1).
constexpr int kCaptionWidth = 46;
constexpr int kCaptionHeight = 32;
constexpr UINT_PTR kViewSubclassId = 1;

std::wstring Utf16FromUtf8(const std::string& text) {
  if (text.empty()) return std::wstring();
  const int length = MultiByteToWideChar(CP_UTF8, 0, text.data(),
                                         static_cast<int>(text.size()),
                                         nullptr, 0);
  std::wstring wide(static_cast<size_t>(length), L'\0');
  MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()),
                      wide.data(), length);
  return wide;
}

const flutter::EncodableValue* Find(const flutter::EncodableMap& map,
                                    const char* key) {
  auto entry = map.find(flutter::EncodableValue(key));
  return entry == map.end() ? nullptr : &entry->second;
}

std::optional<int> IntArg(const flutter::EncodableMap& map, const char* key) {
  const flutter::EncodableValue* value = Find(map, key);
  if (value == nullptr) return std::nullopt;
  if (const auto* number = std::get_if<int32_t>(value)) return *number;
  if (const auto* number = std::get_if<int64_t>(value)) {
    return static_cast<int>(*number);
  }
  return std::nullopt;
}

bool BoolArg(const flutter::EncodableMap& map, const char* key,
             bool fallback) {
  const flutter::EncodableValue* value = Find(map, key);
  if (value == nullptr) return fallback;
  const auto* flag = std::get_if<bool>(value);
  return flag == nullptr ? fallback : *flag;
}

std::string StringArg(const flutter::EncodableMap& map, const char* key) {
  const flutter::EncodableValue* value = Find(map, key);
  if (value == nullptr) return std::string();
  const auto* text = std::get_if<std::string>(value);
  return text == nullptr ? std::string() : *text;
}

}  // namespace

WindowChannel::WindowChannel(HWND window, HWND view,
                             flutter::BinaryMessenger* messenger)
    : window_(window), view_(view) {
  channel_ = std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
      messenger, "dev.opencord/window",
      &flutter::StandardMethodCodec::GetInstance());
  channel_->SetMethodCallHandler([this](const auto& call, auto result) {
    HandleMethodCall(call, std::move(result));
  });
  SetWindowSubclass(view_, ViewProc, kViewSubclassId,
                    reinterpret_cast<DWORD_PTR>(this));
  // Recompute the frame now that WM_NCCALCSIZE is handled.
  SetWindowPos(window_, nullptr, 0, 0, 0, 0,
               SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER |
                   SWP_NOACTIVATE);
}

WindowChannel::~WindowChannel() {
  RemoveWindowSubclass(view_, ViewProc, kViewSubclassId);
}

bool WindowChannel::TakeMaximizeOnShow() {
  const bool maximize = maximize_on_show_;
  maximize_on_show_ = false;
  return maximize;
}

double WindowChannel::Scale() const {
  return GetDpiForWindow(window_) / 96.0;
}

int WindowChannel::FrameX() const {
  const UINT dpi = GetDpiForWindow(window_);
  return GetSystemMetricsForDpi(SM_CXFRAME, dpi) +
         GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
}

int WindowChannel::FrameY() const {
  const UINT dpi = GetDpiForWindow(window_);
  return GetSystemMetricsForDpi(SM_CYFRAME, dpi) +
         GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
}

bool WindowChannel::InMaximizeButton(POINT client_point) const {
  RECT client;
  GetClientRect(window_, &client);
  const double scale = Scale();
  const LONG right = client.right - static_cast<LONG>(kCaptionWidth * scale);
  const LONG left = client.right - static_cast<LONG>(2 * kCaptionWidth * scale);
  return client_point.y >= 0 &&
         client_point.y < static_cast<LONG>(kCaptionHeight * scale) &&
         client_point.x >= left && client_point.x < right;
}

LRESULT WindowChannel::HitTest(POINT screen_point) const {
  POINT point = screen_point;
  ScreenToClient(window_, &point);
  if (!IsZoomed(window_) && point.y >= 0 && point.y < FrameY()) {
    RECT client;
    GetClientRect(window_, &client);
    if (point.x < 2 * FrameX()) return HTTOPLEFT;
    if (point.x >= client.right - 2 * FrameX()) return HTTOPRIGHT;
    return HTTOP;
  }
  if (InMaximizeButton(point)) return HTMAXBUTTON;
  return HTCLIENT;
}

std::optional<LRESULT> WindowChannel::HandleMessage(HWND hwnd, UINT message,
                                                    WPARAM wparam,
                                                    LPARAM lparam) {
  const bool frameless = custom_ && !fullscreen_;
  switch (message) {
    case WM_NCCALCSIZE: {
      if (!frameless || wparam == FALSE) return std::nullopt;
      // Keep the side and bottom borders (they resize, and DWM draws the
      // shadow there); the client area takes the title bar's place.
      auto* params = reinterpret_cast<NCCALCSIZE_PARAMS*>(lparam);
      RECT& rect = params->rgrc[0];
      rect.left += FrameX();
      rect.right -= FrameX();
      rect.bottom -= FrameY();
      if (IsZoomed(hwnd)) rect.top += FrameY();
      return 0;
    }
    case WM_NCHITTEST: {
      if (!frameless) return std::nullopt;
      const LRESULT frame = DefWindowProc(hwnd, message, wparam, lparam);
      if (frame != HTCLIENT) return frame;
      return HitTest({GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
    }
    case WM_NCMOUSEMOVE:
      if (!frameless) return std::nullopt;
      if (wparam == HTMAXBUTTON) {
        if (!tracking_caption_) {
          TRACKMOUSEEVENT track = {sizeof(TRACKMOUSEEVENT)};
          track.dwFlags = TME_LEAVE | TME_NONCLIENT;
          track.hwndTrack = hwnd;
          tracking_caption_ = TrackMouseEvent(&track) != FALSE;
        }
        if (caption_ != CaptionState::kPressed) {
          SetCaptionState(CaptionState::kHover);
        }
      } else {
        SetCaptionState(CaptionState::kNone);
      }
      return std::nullopt;
    case WM_NCMOUSELEAVE:
      tracking_caption_ = false;
      SetCaptionState(CaptionState::kNone);
      return std::nullopt;
    case WM_NCLBUTTONDOWN:
      if (frameless && wparam == HTMAXBUTTON) {
        SetCaptionState(CaptionState::kPressed);
        return 0;
      }
      return std::nullopt;
    case WM_NCLBUTTONUP:
      if (frameless && wparam == HTMAXBUTTON) {
        if (caption_ == CaptionState::kPressed) {
          ShowWindow(hwnd, IsZoomed(hwnd) ? SW_RESTORE : SW_MAXIMIZE);
        }
        SetCaptionState(CaptionState::kHover);
        return 0;
      }
      return std::nullopt;
    case WM_GETMINMAXINFO: {
      auto* info = reinterpret_cast<MINMAXINFO*>(lparam);
      const double scale = Scale();
      info->ptMinTrackSize.x =
          static_cast<LONG>(min_width_ * scale) + 2 * FrameX();
      info->ptMinTrackSize.y =
          static_cast<LONG>(min_height_ * scale) + FrameY();
      return 0;
    }
    case WM_SIZE:
      SendStatus();
      SendGeometry();
      return std::nullopt;
    case WM_MOVE:
      SendGeometry();
      return std::nullopt;
    case WM_ACTIVATE:
      SendStatus();
      return std::nullopt;
    case WM_CLOSE:
      if (intercept_close_) {
        channel_->InvokeMethod("closeRequested", nullptr);
        return 0;
      }
      return std::nullopt;
  }
  return std::nullopt;
}

LRESULT CALLBACK WindowChannel::ViewProc(HWND hwnd, UINT message,
                                         WPARAM wparam, LPARAM lparam,
                                         UINT_PTR, DWORD_PTR data) {
  if (message == WM_NCHITTEST) {
    auto* self = reinterpret_cast<WindowChannel*>(data);
    if (self->custom_ && !self->fullscreen_) {
      const LRESULT hit =
          self->HitTest({GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
      // Let the top-level window answer for the top resize strip and the
      // maximize button, so resizing and snap layouts work there.
      if (hit != HTCLIENT) return HTTRANSPARENT;
    }
  }
  return DefSubclassProc(hwnd, message, wparam, lparam);
}

void WindowChannel::SetCaptionState(CaptionState state) {
  if (caption_ == state) return;
  caption_ = state;
  const char* name = state == CaptionState::kHover     ? "hover"
                     : state == CaptionState::kPressed ? "pressed"
                                                       : "none";
  channel_->InvokeMethod(
      "caption", std::make_unique<flutter::EncodableValue>(std::string(name)));
}

flutter::EncodableValue WindowChannel::Status() const {
  flutter::EncodableMap map;
  map[flutter::EncodableValue("maximized")] =
      flutter::EncodableValue(IsZoomed(window_) != FALSE);
  map[flutter::EncodableValue("fullscreen")] =
      flutter::EncodableValue(fullscreen_);
  map[flutter::EncodableValue("tiled")] = flutter::EncodableValue(false);
  map[flutter::EncodableValue("focused")] =
      flutter::EncodableValue(GetForegroundWindow() == window_);
  return flutter::EncodableValue(map);
}

void WindowChannel::SendStatus() {
  channel_->InvokeMethod("status",
                         std::make_unique<flutter::EncodableValue>(Status()));
}

void WindowChannel::SendGeometry() {
  if (IsZoomed(window_) || IsIconic(window_) || fullscreen_) return;
  RECT rect;
  GetWindowRect(window_, &rect);
  const double scale = Scale();
  flutter::EncodableMap map;
  map[flutter::EncodableValue("width")] = flutter::EncodableValue(
      static_cast<int>((rect.right - rect.left) / scale));
  map[flutter::EncodableValue("height")] = flutter::EncodableValue(
      static_cast<int>((rect.bottom - rect.top) / scale));
  map[flutter::EncodableValue("x")] = flutter::EncodableValue(
      static_cast<int>(rect.left / scale));
  map[flutter::EncodableValue("y")] = flutter::EncodableValue(
      static_cast<int>(rect.top / scale));
  channel_->InvokeMethod("geometry",
                         std::make_unique<flutter::EncodableValue>(map));
}

void WindowChannel::SetFullscreen(bool fullscreen) {
  if (fullscreen == fullscreen_) return;
  if (fullscreen) {
    saved_style_ = GetWindowLong(window_, GWL_STYLE);
    GetWindowPlacement(window_, &saved_placement_);
    MONITORINFO monitor = {sizeof(MONITORINFO)};
    GetMonitorInfo(MonitorFromWindow(window_, MONITOR_DEFAULTTONEAREST),
                   &monitor);
    fullscreen_ = true;
    SetWindowLong(window_, GWL_STYLE,
                  saved_style_ & ~static_cast<LONG>(WS_OVERLAPPEDWINDOW));
    const RECT& area = monitor.rcMonitor;
    SetWindowPos(window_, HWND_TOP, area.left, area.top,
                 area.right - area.left, area.bottom - area.top,
                 SWP_NOOWNERZORDER | SWP_FRAMECHANGED);
  } else {
    fullscreen_ = false;
    SetWindowLong(window_, GWL_STYLE, saved_style_);
    SetWindowPlacement(window_, &saved_placement_);
    SetWindowPos(window_, nullptr, 0, 0, 0, 0,
                 SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER |
                     SWP_FRAMECHANGED);
  }
  SendStatus();
}

void WindowChannel::ShowSystemMenu() {
  HMENU menu = GetSystemMenu(window_, FALSE);
  if (menu == nullptr) return;
  const bool maximized = IsZoomed(window_) != FALSE;
  EnableMenuItem(menu, SC_RESTORE, maximized ? MF_ENABLED : MF_GRAYED);
  EnableMenuItem(menu, SC_MOVE, maximized ? MF_GRAYED : MF_ENABLED);
  EnableMenuItem(menu, SC_SIZE, maximized ? MF_GRAYED : MF_ENABLED);
  EnableMenuItem(menu, SC_MAXIMIZE, maximized ? MF_GRAYED : MF_ENABLED);
  POINT cursor;
  GetCursorPos(&cursor);
  const BOOL command = TrackPopupMenu(
      menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, cursor.x, cursor.y, 0, window_,
      nullptr);
  if (command != 0) {
    PostMessage(window_, WM_SYSCOMMAND, static_cast<WPARAM>(command), 0);
  }
}

flutter::EncodableValue WindowChannel::Configure(
    const flutter::EncodableMap& args) {
  custom_ = StringArg(args, "chrome") != "system";
  intercept_close_ = BoolArg(args, "interceptClose", false);
  min_width_ = IntArg(args, "minWidth").value_or(min_width_);
  min_height_ = IntArg(args, "minHeight").value_or(min_height_);

  const double scale = Scale();
  const int width = IntArg(args, "width").value_or(1280);
  const int height = IntArg(args, "height").value_or(800);
  RECT rect = {0, 0, static_cast<LONG>(width * scale),
               static_cast<LONG>(height * scale)};
  const std::optional<int> x = IntArg(args, "x");
  const std::optional<int> y = IntArg(args, "y");
  bool placed = false;
  if (x.has_value() && y.has_value()) {
    OffsetRect(&rect, static_cast<LONG>(*x * scale),
               static_cast<LONG>(*y * scale));
    // A position on a monitor that is gone falls back to the primary one.
    placed = MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) != nullptr;
  }
  if (!placed) {
    MONITORINFO monitor = {sizeof(MONITORINFO)};
    GetMonitorInfo(MonitorFromPoint({0, 0}, MONITOR_DEFAULTTOPRIMARY),
                   &monitor);
    const RECT& area = monitor.rcWork;
    const LONG w = rect.right - rect.left;
    const LONG h = rect.bottom - rect.top;
    rect.left = area.left + (area.right - area.left - w) / 2;
    rect.top = area.top + (area.bottom - area.top - h) / 2;
    rect.right = rect.left + w;
    rect.bottom = rect.top + h;
  }
  SetWindowPos(window_, nullptr, rect.left, rect.top, rect.right - rect.left,
               rect.bottom - rect.top,
               SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
  maximize_on_show_ = BoolArg(args, "maximized", false);

  flutter::EncodableMap info;
  info[flutter::EncodableValue("chrome")] =
      flutter::EncodableValue(std::string(custom_ ? "custom" : "system"));
  info[flutter::EncodableValue("transparent")] = flutter::EncodableValue(false);
  info[flutter::EncodableValue("frameMargin")] = flutter::EncodableValue(0);
  info[flutter::EncodableValue("buttonLayout")] =
      flutter::EncodableValue(std::string());
  info[flutter::EncodableValue("status")] = Status();
  return flutter::EncodableValue(info);
}

void WindowChannel::HandleMethodCall(
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  const std::string& method = call.method_name();
  const flutter::EncodableValue* args = call.arguments();
  if (method == "configure") {
    const auto* map = args == nullptr
                          ? nullptr
                          : std::get_if<flutter::EncodableMap>(args);
    if (map == nullptr) {
      result->Error("bad-arguments", "configure takes a map");
      return;
    }
    result->Success(Configure(*map));
    return;
  }
  if (method == "setTitle") {
    const auto* title =
        args == nullptr ? nullptr : std::get_if<std::string>(args);
    if (title != nullptr) SetWindowText(window_, Utf16FromUtf8(*title).c_str());
  } else if (method == "startDrag") {
    ReleaseCapture();
    SendMessage(window_, WM_SYSCOMMAND, SC_MOVE | HTCAPTION, 0);
  } else if (method == "showWindowMenu") {
    ShowSystemMenu();
  } else if (method == "minimize") {
    ShowWindow(window_, SW_MINIMIZE);
  } else if (method == "toggleMaximize") {
    ShowWindow(window_, IsZoomed(window_) ? SW_RESTORE : SW_MAXIMIZE);
  } else if (method == "setFullscreen") {
    const auto* flag = args == nullptr ? nullptr : std::get_if<bool>(args);
    if (flag != nullptr) SetFullscreen(*flag);
  } else if (method == "close") {
    PostMessage(window_, WM_CLOSE, 0, 0);
  } else if (method == "quit") {
    intercept_close_ = false;
    PostMessage(window_, WM_CLOSE, 0, 0);
  } else if (method == "hide") {
    ShowWindow(window_, SW_HIDE);
  } else if (method == "show") {
    ShowWindow(window_, IsIconic(window_) ? SW_RESTORE : SW_SHOW);
    SetForegroundWindow(window_);
  } else if (method == "setUrgent") {
    const auto* flag = args == nullptr ? nullptr : std::get_if<bool>(args);
    FLASHWINFO flash = {sizeof(FLASHWINFO)};
    flash.hwnd = window_;
    flash.dwFlags = flag != nullptr && *flag ? FLASHW_TRAY | FLASHW_TIMERNOFG
                                             : FLASHW_STOP;
    FlashWindowEx(&flash);
  } else if (method == "status") {
    result->Success(Status());
    return;
  } else if (method == "startResize") {
    // Windows resizes from its own borders.
  } else {
    result->NotImplemented();
    return;
  }
  result->Success();
}
