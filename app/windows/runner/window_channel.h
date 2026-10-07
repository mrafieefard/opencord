#ifndef RUNNER_WINDOW_CHANNEL_H_
#define RUNNER_WINDOW_CHANNEL_H_

#include <flutter/binary_messenger.h>
#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>
#include <windows.h>

#include <memory>
#include <optional>
#include <string>

// The "dev.opencord/window" method channel (desktop UI plan §3.1): a
// frameless window that keeps Aero Snap, Windows 11 snap layouts, native
// resize borders and the DWM shadow, with the title bar drawn by the app.
class WindowChannel {
 public:
  WindowChannel(HWND window, HWND view, flutter::BinaryMessenger* messenger);
  ~WindowChannel();

  WindowChannel(const WindowChannel&) = delete;
  WindowChannel& operator=(const WindowChannel&) = delete;

  // Handles the top-level window's messages that shape the frame. Returns
  // nothing for messages left to the default handling.
  std::optional<LRESULT> HandleMessage(HWND hwnd, UINT message, WPARAM wparam,
                                       LPARAM lparam);

  // Whether the window should open maximized (from the saved geometry).
  bool TakeMaximizeOnShow();

  // Brings the window back, from the tray or from behind other windows, and
  // passes `link` (an opencord:// link, or "") to the app. A later launch
  // of the app lands here (desktop UI plan §15).
  void Present(const std::wstring& link);

 private:
  enum class CaptionState { kNone, kHover, kPressed };

  static LRESULT CALLBACK ViewProc(HWND hwnd, UINT message, WPARAM wparam,
                                   LPARAM lparam, UINT_PTR subclass_id,
                                   DWORD_PTR data);

  void HandleMethodCall(
      const flutter::MethodCall<flutter::EncodableValue>& call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
  flutter::EncodableValue Configure(const flutter::EncodableMap& args);
  LRESULT HitTest(POINT screen_point) const;
  int FrameX() const;
  int FrameY() const;
  double Scale() const;
  bool InMaximizeButton(POINT client_point) const;
  void SetCaptionState(CaptionState state);
  void SetFullscreen(bool fullscreen);
  void ShowSystemMenu();
  flutter::EncodableValue Status() const;
  void SendStatus();
  void SendGeometry();

  HWND window_;
  HWND view_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
  bool custom_ = true;
  bool intercept_close_ = false;
  bool fullscreen_ = false;
  bool maximize_on_show_ = false;
  bool tracking_caption_ = false;
  LONG saved_style_ = 0;
  WINDOWPLACEMENT saved_placement_ = {sizeof(WINDOWPLACEMENT)};
  int min_width_ = 940;
  int min_height_ = 560;
  CaptionState caption_ = CaptionState::kNone;
};

#endif  // RUNNER_WINDOW_CHANNEL_H_
