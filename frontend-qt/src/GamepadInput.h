// GamepadInput — small XInput polling adapter for the Qt frontend.
#pragma once

#include <array>

class GamepadInput {
public:
    struct Snapshot {
        std::array<bool, 6> buttons{}; // Up, Down, Left, Right, A, B
        bool connected = false;
    };

    GamepadInput();
    ~GamepadInput();

    GamepadInput(const GamepadInput &) = delete;
    GamepadInput &operator=(const GamepadInput &) = delete;

    Snapshot poll() const;

private:
    void *m_library = nullptr;
    void *m_getState = nullptr;
};
