#include "tray_channel.h"

#include <flutter/standard_method_codec.h>
#include <shellapi.h>
#include <windowsx.h>

#include <cstdint>
#include <cstdlib>
#include <string>
#include <utility>
#include <vector>

namespace {

// The icon's messages come to the window as this.
constexpr UINT kTrayMessage = WM_APP + 0x51;
constexpr UINT kTrayId = 1;

std::wstring Utf16FromUtf8(const std::string& utf8) {
  if (utf8.empty()) return std::wstring();
  const int length = ::MultiByteToWideChar(CP_UTF8, 0, utf8.data(),
                                           static_cast<int>(utf8.size()),
                                           nullptr, 0);
  std::wstring utf16(static_cast<size_t>(length), L'\0');
  ::MultiByteToWideChar(CP_UTF8, 0, utf8.data(), static_cast<int>(utf8.size()),
                        utf16.data(), length);
  return utf16;
}

const flutter::EncodableValue* Find(const flutter::EncodableMap& map,
                                    const char* key) {
  auto it = map.find(flutter::EncodableValue(key));
  return it == map.end() ? nullptr : &it->second;
}

bool BoolOf(const flutter::EncodableMap& map, const char* key) {
  const auto* value = Find(map, key);
  const auto* flag = value ? std::get_if<bool>(value) : nullptr;
  return flag != nullptr && *flag;
}

std::string StringOf(const flutter::EncodableMap& map, const char* key) {
  const auto* value = Find(map, key);
  const auto* text = value ? std::get_if<std::string>(value) : nullptr;
  return text ? *text : std::string();
}

int IntOf(const flutter::EncodableMap& map, const char* key) {
  const auto* value = Find(map, key);
  if (value == nullptr) return 0;
  if (const auto* small = std::get_if<int32_t>(value)) return *small;
  if (const auto* large = std::get_if<int64_t>(value)) {
    return static_cast<int>(*large);
  }
  return 0;
}

// The icon nearest the size the notification area wants, as an HICON.
HICON IconFrom(const flutter::EncodableList& icons, int wanted) {
  const flutter::EncodableMap* best = nullptr;
  int best_size = 0;
  for (const auto& entry : icons) {
    const auto* icon = std::get_if<flutter::EncodableMap>(&entry);
    if (icon == nullptr) continue;
    const int size = IntOf(*icon, "size");
    if (size <= 0) continue;
    if (best == nullptr || std::abs(size - wanted) < std::abs(best_size - wanted)) {
      best = icon;
      best_size = size;
    }
  }
  if (best == nullptr) return nullptr;
  const auto* value = Find(*best, "rgba");
  const auto* rgba = value ? std::get_if<std::vector<uint8_t>>(value) : nullptr;
  const size_t pixels = static_cast<size_t>(best_size) * best_size;
  if (rgba == nullptr || rgba->size() != pixels * 4) return nullptr;

  BITMAPV5HEADER header = {};
  header.bV5Size = sizeof(header);
  header.bV5Width = best_size;
  header.bV5Height = -best_size;  // Top-down, as the app draws it.
  header.bV5Planes = 1;
  header.bV5BitCount = 32;
  header.bV5Compression = BI_BITFIELDS;
  header.bV5RedMask = 0x00FF0000;
  header.bV5GreenMask = 0x0000FF00;
  header.bV5BlueMask = 0x000000FF;
  header.bV5AlphaMask = 0xFF000000;
  void* bits = nullptr;
  HDC screen = ::GetDC(nullptr);
  HBITMAP color = ::CreateDIBSection(
      screen, reinterpret_cast<BITMAPINFO*>(&header), DIB_RGB_COLORS, &bits,
      nullptr, 0);
  ::ReleaseDC(nullptr, screen);
  if (color == nullptr || bits == nullptr) return nullptr;
  auto* out = static_cast<uint8_t*>(bits);
  for (size_t i = 0; i < pixels; ++i) {
    // RGBA in, BGRA out; icons take straight alpha.
    out[i * 4] = (*rgba)[i * 4 + 2];
    out[i * 4 + 1] = (*rgba)[i * 4 + 1];
    out[i * 4 + 2] = (*rgba)[i * 4];
    out[i * 4 + 3] = (*rgba)[i * 4 + 3];
  }
  HBITMAP mask = ::CreateBitmap(best_size, best_size, 1, 1, nullptr);
  ICONINFO info = {};
  info.fIcon = TRUE;
  info.hbmColor = color;
  info.hbmMask = mask;
  HICON icon = ::CreateIconIndirect(&info);
  ::DeleteObject(color);
  ::DeleteObject(mask);
  return icon;
}

// The app's menu items as a popup menu, submenus included.
HMENU MenuFrom(const flutter::EncodableList& items) {
  HMENU menu = ::CreatePopupMenu();
  UINT position = 0;
  for (const auto& entry : items) {
    const auto* item = std::get_if<flutter::EncodableMap>(&entry);
    if (item == nullptr) continue;
    if (BoolOf(*item, "separator")) {
      ::AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
      ++position;
      continue;
    }
    std::wstring label = Utf16FromUtf8(StringOf(*item, "label"));
    MENUITEMINFOW info = {};
    info.cbSize = sizeof(info);
    info.fMask = MIIM_ID | MIIM_STRING | MIIM_FTYPE | MIIM_STATE;
    info.fType = MFT_STRING;
    info.wID = static_cast<UINT>(IntOf(*item, "id"));
    info.dwTypeData = label.data();
    const auto* checked = Find(*item, "checked");
    if (checked != nullptr && std::holds_alternative<bool>(*checked)) {
      if (BoolOf(*item, "radio")) info.fType |= MFT_RADIOCHECK;
      if (std::get<bool>(*checked)) info.fState = MFS_CHECKED;
    }
    const auto* children = Find(*item, "children");
    const auto* list =
        children ? std::get_if<flutter::EncodableList>(children) : nullptr;
    if (list != nullptr && !list->empty()) {
      info.fMask |= MIIM_SUBMENU;
      info.hSubMenu = MenuFrom(*list);
    }
    ::InsertMenuItemW(menu, position++, TRUE, &info);
  }
  return menu;
}

}  // namespace

TrayChannel::TrayChannel(HWND window, flutter::BinaryMessenger* messenger,
                         std::function<void()> present)
    : window_(window),
      channel_(std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          messenger, "dev.opencord/tray",
          &flutter::StandardMethodCodec::GetInstance())),
      present_(std::move(present)),
      taskbar_created_(::RegisterWindowMessageW(L"TaskbarCreated")) {
  channel_->SetMethodCallHandler([this](const auto& call, auto result) {
    HandleMethodCall(call, std::move(result));
  });
}

TrayChannel::~TrayChannel() {
  channel_->SetMethodCallHandler(nullptr);
  Remove();
}

void TrayChannel::HandleMethodCall(
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  const std::string& method = call.method_name();
  if (method == "show") {
    const auto* args = std::get_if<flutter::EncodableMap>(call.arguments());
    result->Success(flutter::EncodableValue(args != nullptr && Show(*args)));
  } else if (method == "close") {
    Remove();
    result->Success();
  } else {
    result->NotImplemented();
  }
}

bool TrayChannel::Show(const flutter::EncodableMap& args) {
  const auto* icons_value = Find(args, "icons");
  const auto* icons =
      icons_value ? std::get_if<flutter::EncodableList>(icons_value) : nullptr;
  if (icons != nullptr) {
    const UINT dpi = ::GetDpiForWindow(window_);
    HICON icon = IconFrom(*icons, ::GetSystemMetricsForDpi(SM_CXSMICON, dpi));
    if (icon != nullptr) {
      if (icon_ != nullptr) ::DestroyIcon(icon_);
      icon_ = icon;
    }
  }
  tooltip_ = Utf16FromUtf8(StringOf(args, "tooltip"));
  const auto* menu = Find(args, "menu");
  if (menu != nullptr) {
    if (const auto* items = std::get_if<flutter::EncodableList>(menu)) {
      menu_ = *items;
    }
  }
  return Add();
}

bool TrayChannel::Add() {
  if (icon_ == nullptr) return false;
  NOTIFYICONDATAW data = {};
  data.cbSize = sizeof(data);
  data.hWnd = window_;
  data.uID = kTrayId;
  data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
  data.uCallbackMessage = kTrayMessage;
  data.hIcon = icon_;
  const std::wstring tip = L"Opencord" + (tooltip_.empty() ? std::wstring()
                                                           : L"\n" + tooltip_);
  wcsncpy_s(data.szTip, tip.c_str(), _TRUNCATE);
  if (added_ && ::Shell_NotifyIconW(NIM_MODIFY, &data)) return true;
  if (!::Shell_NotifyIconW(NIM_ADD, &data)) {
    added_ = false;
    return false;
  }
  data.uVersion = NOTIFYICON_VERSION_4;
  ::Shell_NotifyIconW(NIM_SETVERSION, &data);
  added_ = true;
  return true;
}

void TrayChannel::Remove() {
  if (added_) {
    NOTIFYICONDATAW data = {};
    data.cbSize = sizeof(data);
    data.hWnd = window_;
    data.uID = kTrayId;
    ::Shell_NotifyIconW(NIM_DELETE, &data);
    added_ = false;
  }
  if (icon_ != nullptr) {
    ::DestroyIcon(icon_);
    icon_ = nullptr;
  }
}

void TrayChannel::ShowMenu(POINT at) {
  HMENU menu = MenuFrom(menu_);
  // Without this the menu stays open after a click elsewhere.
  ::SetForegroundWindow(window_);
  const UINT flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY |
                     (::GetSystemMetrics(SM_MENUDROPALIGNMENT) ? TPM_RIGHTALIGN
                                                               : TPM_LEFTALIGN);
  const BOOL picked =
      ::TrackPopupMenuEx(menu, flags, at.x, at.y, window_, nullptr);
  ::PostMessageW(window_, WM_NULL, 0, 0);
  ::DestroyMenu(menu);
  if (picked > 0) {
    channel_->InvokeMethod("menu", std::make_unique<flutter::EncodableValue>(
                                       static_cast<int32_t>(picked)));
  }
}

std::optional<LRESULT> TrayChannel::HandleMessage(HWND, UINT message,
                                                  WPARAM wparam,
                                                  LPARAM lparam) {
  if (message == taskbar_created_ && taskbar_created_ != 0) {
    // Explorer started again and forgot the icon.
    added_ = false;
    Add();
    return std::nullopt;
  }
  if (message != kTrayMessage) return std::nullopt;
  switch (LOWORD(lparam)) {
    case NIN_SELECT:
    case NIN_KEYSELECT:
      // Brought forward here, while the click still gives this process the
      // right to take the foreground.
      present_();
      channel_->InvokeMethod("activate", nullptr);
      break;
    case WM_CONTEXTMENU:
      ShowMenu({GET_X_LPARAM(wparam), GET_Y_LPARAM(wparam)});
      break;
    default:
      break;
  }
  return 0;
}
