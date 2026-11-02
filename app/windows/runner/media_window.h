#ifndef RUNNER_MEDIA_WINDOW_H_
#define RUNNER_MEDIA_WINDOW_H_

#include <flutter/binary_messenger.h>
#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>
#include <flutter_windows.h>
#include <windows.h>

#include <cstdint>
#include <memory>
#include <optional>

// The "dev.opencord/media_window" method channel (D86): the media viewer's
// own window, a second Flutter view on the app's engine in a see-through
// top-level window with a taskbar button of its own. It covers the main
// window's monitor, taskbar included, or is a normal window with the app's
// own frame (side and bottom borders, DWM shadow). The app draws its title
// buttons.
class MediaWindowChannel {
 public:
  MediaWindowChannel(HWND main, flutter::BinaryMessenger* messenger);
  ~MediaWindowChannel();

  MediaWindowChannel(const MediaWindowChannel&) = delete;
  MediaWindowChannel& operator=(const MediaWindowChannel&) = delete;

 private:
  static LRESULT CALLBACK WndProc(HWND hwnd, UINT message, WPARAM wparam,
                                  LPARAM lparam);
  static LRESULT CALLBACK ViewProc(HWND hwnd, UINT message, WPARAM wparam,
                                   LPARAM lparam, UINT_PTR subclass_id,
                                   DWORD_PTR data);

  LRESULT HandleMessage(HWND hwnd, UINT message, WPARAM wparam,
                        LPARAM lparam);
  void HandleMethodCall(
      const flutter::MethodCall<flutter::EncodableValue>& call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

  // Opens the window; answers the new view's id, or nothing if it could not
  // be made.
  std::optional<int64_t> Open(const flutter::EncodableMap& args);
  void Close();
  void SetFullscreen(bool fullscreen);
  void LayoutView();
  LRESULT HitTest(POINT screen_point) const;
  double Scale() const;
  int FrameX() const;
  int FrameY() const;
  void SendStatus();
  void SendGeometry();

  HWND main_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
  // The viewer's window and view while it is open.
  HWND window_ = nullptr;
  FlutterDesktopViewControllerRef controller_ = nullptr;
  HWND view_ = nullptr;
  bool fullscreen_ = true;
  // The normal window's place, in physical pixels.
  RECT normal_ = {};
};

#endif  // RUNNER_MEDIA_WINDOW_H_
