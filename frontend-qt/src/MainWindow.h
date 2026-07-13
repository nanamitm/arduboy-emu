// MainWindow — top-level window: display, menus, input, audio, status bar.
#pragma once

#include <array>
#include <QElapsedTimer>
#include <QMainWindow>
#include <QVector>

#include "EmulatorCore.h"
#include "GamepadInput.h"

class QTimer;
class QLabel;
class QPlainTextEdit;
class QDockWidget;
class DisplayWidget;
class AudioOutput;

class MainWindow : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(QWidget *parent = nullptr);
    ~MainWindow() override;

    // Load a ROM given on the command line, after the window is shown.
    void openPath(const QString &path);

protected:
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void closeEvent(QCloseEvent *event) override;
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dropEvent(QDropEvent *event) override;

private slots:
    void tick();          // one emulated frame
    void openRom();
    void reloadRom();
    void reset();
    void togglePause();
    void takeScreenshot();
    void toggleGif();
    void saveState();
    void loadState();
    void toggleMute();
    void toggleSmooth();
    void toggleRegisters();
    void toggleFullscreen();
    void setScale(int factor);
    void setSkin(int skin);
    void about();

private:
    enum class InputSource { Keyboard, Skin, Gamepad };

    void buildMenus();
    void updateStatus();
    void setPaused(bool paused);
    void setButtonSource(EmulatorCore::Button button, bool pressed, InputSource source);
    void pollGamepad();
    bool mapButton(int key, EmulatorCore::Button &out) const;
    // True if `path` looks like a ROM we can load (.hex / .arduboy / .elf).
    static bool isSupportedRom(const QString &path);

    EmulatorCore m_core;
    DisplayWidget *m_display = nullptr;
    AudioOutput *m_audio = nullptr;
    QTimer *m_timer = nullptr;
    GamepadInput m_gamepad;
    QVector<float> m_audioBuf;

    QDockWidget *m_regDock = nullptr;
    QPlainTextEdit *m_regView = nullptr;

    // Status-bar widgets
    QLabel *m_fpsLabel = nullptr;
    QLabel *m_cpuLabel = nullptr;
    QLabel *m_ledLabel = nullptr;
    QLabel *m_stateLabel = nullptr;

    QString m_romPath;
    bool m_paused = false;
    int m_scale = 4;
    std::array<bool, 6> m_keyboardButtons{};
    std::array<bool, 6> m_skinButtons{};
    std::array<bool, 6> m_gamepadButtons{};
    bool m_gamepadConnected = false;

    // FPS measurement
    QElapsedTimer m_fpsClock;
    int m_frameCounter = 0;
    double m_fps = 0.0;
};
