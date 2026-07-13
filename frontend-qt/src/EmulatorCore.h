// EmulatorCore — thin C++ wrapper around the arduboy_ffi C ABI.
//
// Owns the opaque `Emu*` handle and exposes typed helpers used by the GUI.
// All calls forward to arduboy_ffi.dll; this class holds no emulation logic.
#pragma once

#include <QImage>
#include <QString>
#include <QVector>

struct Emu; // opaque, from arduboy_ffi.h

class EmulatorCore {
public:
    // Arduboy buttons, matching enum AbButton in arduboy_ffi.h.
    enum Button { Up = 0, Down = 1, Left = 2, Right = 3, A = 4, B = 5 };

    EmulatorCore();
    ~EmulatorCore();

    EmulatorCore(const EmulatorCore &) = delete;
    EmulatorCore &operator=(const EmulatorCore &) = delete;

    // Load a ROM (.hex / .arduboy / .elf). Returns true on success.
    bool loadFile(const QString &path);
    QString lastError() const;
    bool isLoaded() const { return m_loaded; }

    void reset();
    void runFrame();
    void setButton(Button b, bool pressed);

    // Copy the current 128x64 RGBA framebuffer into `img` (kept as a member so
    // we reuse its allocation across frames).
    const QImage &frame();

    // Render this frame's audio into an interleaved L,R float buffer.
    // Returns the number of stereo pairs written.
    int renderAudio(QVector<float> &out, unsigned int sampleRate, float volume);

    // LEDs & serial
    void ledRgb(int &r, int &g, int &b) const;
    bool ledTx() const;
    bool ledRx() const;
    QString takeSerial();

    // EEPROM / save state
    bool eepromDirty() const;
    bool saveEeprom();
    bool saveState();
    bool loadState();

    // Screenshot / GIF
    bool screenshotPng(const QString &path);
    bool gifStart(const QString &path);
    bool gifStop();
    bool gifRecording() const;

    // Debug
    QString dumpRegs();

    int cpuType() const; // 0 = 32u4, 1 = 328P
    QString title() const;

    static int screenWidth();
    static int screenHeight();

private:
    Emu *m_h = nullptr;
    bool m_loaded = false;
    QImage m_image;      // reused RGBA framebuffer view
    QVector<char> m_txt; // reused text scratch for debug dumps
};
