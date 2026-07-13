#include "GamepadInput.h"

#ifdef _WIN32
#include <windows.h>
#include <xinput.h>
#endif

GamepadInput::GamepadInput() {
#ifdef _WIN32
    // XInput 1.4 ships with current Windows; 1.3 covers older DirectX installs.
    const wchar_t *libraries[] = {L"xinput1_4.dll", L"xinput1_3.dll", L"xinput9_1_0.dll"};
    for (const wchar_t *name : libraries) {
        HMODULE library = LoadLibraryW(name);
        if (!library)
            continue;
        auto getState = GetProcAddress(library, "XInputGetState");
        if (getState) {
            m_library = library;
            m_getState = reinterpret_cast<void *>(getState);
            break;
        }
        FreeLibrary(library);
    }
#endif
}

GamepadInput::~GamepadInput() {
#ifdef _WIN32
    if (m_library)
        FreeLibrary(static_cast<HMODULE>(m_library));
#endif
}

GamepadInput::Snapshot GamepadInput::poll() const {
    Snapshot result;
#ifdef _WIN32
    if (!m_getState)
        return result;

    using GetState = DWORD(WINAPI *)(DWORD, XINPUT_STATE *);
    const auto getState = reinterpret_cast<GetState>(m_getState);
    for (DWORD index = 0; index < XUSER_MAX_COUNT; ++index) {
        XINPUT_STATE state{};
        if (getState(index, &state) != ERROR_SUCCESS)
            continue;

        const XINPUT_GAMEPAD &pad = state.Gamepad;
        constexpr SHORT kStickThreshold = 16000;
        result.connected = true;
        result.buttons[0] = (pad.wButtons & XINPUT_GAMEPAD_DPAD_UP) || pad.sThumbLY > kStickThreshold;
        result.buttons[1] = (pad.wButtons & XINPUT_GAMEPAD_DPAD_DOWN) || pad.sThumbLY < -kStickThreshold;
        result.buttons[2] = (pad.wButtons & XINPUT_GAMEPAD_DPAD_LEFT) || pad.sThumbLX < -kStickThreshold;
        result.buttons[3] = (pad.wButtons & XINPUT_GAMEPAD_DPAD_RIGHT) || pad.sThumbLX > kStickThreshold;
        // Standard layout: bottom/west face buttons act as Arduboy A; right/north as B.
        result.buttons[4] = pad.wButtons & (XINPUT_GAMEPAD_A | XINPUT_GAMEPAD_X);
        result.buttons[5] = pad.wButtons & (XINPUT_GAMEPAD_B | XINPUT_GAMEPAD_Y);
        return result; // Use the first connected controller.
    }
#endif
    return result;
}
