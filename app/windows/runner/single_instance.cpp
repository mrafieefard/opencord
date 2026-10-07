#include "single_instance.h"

#include <shellapi.h>

#include <cwchar>
#include <initializer_list>
#include <utility>

namespace single_instance {

namespace {

constexpr wchar_t kMutexName[] = L"Local\\dev.opencord.opencord";
constexpr wchar_t kSchemeKey[] = L"Software\\Classes\\opencord";
constexpr wchar_t kCommandKey[] =
    L"Software\\Classes\\opencord\\shell\\open\\command";

void SetString(HKEY key, const wchar_t* name, const std::wstring& value) {
  ::RegSetValueExW(key, name, 0, REG_SZ,
                   reinterpret_cast<const BYTE*>(value.c_str()),
                   static_cast<DWORD>((value.size() + 1) * sizeof(wchar_t)));
}

bool SetKeyStrings(
    const wchar_t* path,
    std::initializer_list<std::pair<const wchar_t*, std::wstring>> values) {
  HKEY key = nullptr;
  if (::RegCreateKeyExW(HKEY_CURRENT_USER, path, 0, nullptr, 0, KEY_WRITE,
                        nullptr, &key, nullptr) != ERROR_SUCCESS) {
    return false;
  }
  for (const auto& [name, value] : values) {
    SetString(key, name, value);
  }
  ::RegCloseKey(key);
  return true;
}

}  // namespace

std::wstring LinkArgument() {
  int count = 0;
  wchar_t** arguments = ::CommandLineToArgvW(::GetCommandLineW(), &count);
  if (arguments == nullptr) return L"";
  std::wstring link;
  for (int i = 1; i < count; i++) {
    if (::_wcsnicmp(arguments[i], L"opencord:", 9) == 0) {
      link = arguments[i];
      break;
    }
  }
  ::LocalFree(arguments);
  return link;
}

void RegisterLinkScheme() {
  wchar_t path[MAX_PATH];
  const DWORD length = ::GetModuleFileNameW(nullptr, path, MAX_PATH);
  if (length == 0 || length == MAX_PATH) return;
  const std::wstring executable(path, length);
  if (!SetKeyStrings(kSchemeKey,
                     {{nullptr, L"URL:Opencord"}, {L"URL Protocol", L""}})) {
    return;
  }
  SetKeyStrings(kCommandKey, {{nullptr, L"\"" + executable + L"\" \"%1\""}});
}

bool HandOverToRunning(const std::wstring& link) {
  // Kept for the life of the process; Windows releases it on exit.
  HANDLE mutex = ::CreateMutexW(nullptr, FALSE, kMutexName);
  if (mutex == nullptr || ::GetLastError() != ERROR_ALREADY_EXISTS) {
    return false;
  }
  // The running one may still be creating its window.
  HWND running = nullptr;
  for (int attempt = 0; attempt < 50 && running == nullptr; attempt++) {
    running = ::FindWindowW(kWindowClass, nullptr);
    if (running == nullptr) ::Sleep(100);
  }
  if (running == nullptr) return true;
  // Lets the running instance come to the front: this launch was the
  // user's, so it may hand that right on.
  DWORD process = 0;
  ::GetWindowThreadProcessId(running, &process);
  ::AllowSetForegroundWindow(process);
  COPYDATASTRUCT data = {};
  data.dwData = kLinkMessage;
  data.cbData = static_cast<DWORD>((link.size() + 1) * sizeof(wchar_t));
  data.lpData = const_cast<wchar_t*>(link.c_str());
  ::SendMessageTimeoutW(running, WM_COPYDATA, 0,
                        reinterpret_cast<LPARAM>(&data), SMTO_ABORTIFHUNG, 2000,
                        nullptr);
  return true;
}

}  // namespace single_instance
