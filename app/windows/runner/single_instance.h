#ifndef RUNNER_SINGLE_INSTANCE_H_
#define RUNNER_SINGLE_INSTANCE_H_

#include <windows.h>

#include <string>

// One Opencord per user session (desktop UI plan §15): a later launch hands
// its opencord:// link to the running one and quits.
namespace single_instance {

// The window class of Opencord's main window, which later launches look
// for.
inline constexpr wchar_t kWindowClass[] = L"OPENCORD_MAIN_WINDOW";

// Tags the WM_COPYDATA that carries a handed-over link.
inline constexpr ULONG_PTR kLinkMessage = 0x4F434C4B;  // "OCLK"

// The opencord:// link among this launch's arguments, or "".
std::wstring LinkArgument();

// Registers opencord:// for the current user, pointing at this executable,
// as other chat apps do on start.
void RegisterLinkScheme();

// True when Opencord is already running: it has been given `link` (or just
// brought forward) and this launch should quit.
bool HandOverToRunning(const std::wstring& link);

}  // namespace single_instance

#endif  // RUNNER_SINGLE_INSTANCE_H_
