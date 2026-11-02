#ifndef RUNNER_TRAY_CHANNEL_H_
#define RUNNER_TRAY_CHANNEL_H_

#include <flutter/binary_messenger.h>
#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>
#include <windows.h>

#include <functional>
#include <memory>
#include <optional>
#include <string>

// The "dev.opencord/tray" method channel (desktop UI plan §15): the icon
// in the notification area and its menu. The app draws the icon and lays
// out the menu; this shows them, brings the window forward on a click and
// sends back "activate", and sends "menu" with the id of a picked item.
class TrayChannel {
 public:
  // `present` brings the window back and forward.
  TrayChannel(HWND window, flutter::BinaryMessenger* messenger,
              std::function<void()> present);
  ~TrayChannel();

  TrayChannel(const TrayChannel&) = delete;
  TrayChannel& operator=(const TrayChannel&) = delete;

  // Handles the icon's messages, and Explorer starting again. Returns
  // nothing for every other message.
  std::optional<LRESULT> HandleMessage(HWND hwnd, UINT message, WPARAM wparam,
                                       LPARAM lparam);

 private:
  void HandleMethodCall(
      const flutter::MethodCall<flutter::EncodableValue>& call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
  bool Show(const flutter::EncodableMap& args);
  bool Add();
  void Remove();
  void ShowMenu(POINT at);

  HWND window_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
  std::function<void()> present_;
  UINT taskbar_created_;
  bool added_ = false;
  HICON icon_ = nullptr;
  std::wstring tooltip_;
  flutter::EncodableList menu_;
};

#endif  // RUNNER_TRAY_CHANNEL_H_
