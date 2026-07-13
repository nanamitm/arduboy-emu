#include "EmulatorCore.h"

#include "arduboy_ffi.h"

#include <cstring>

EmulatorCore::EmulatorCore() {
    m_h = abemu_new();
    m_image = QImage(screenWidth(), screenHeight(), QImage::Format_RGBA8888);
    m_image.fill(Qt::black);
    m_txt.resize(4096);
}

EmulatorCore::~EmulatorCore() {
    if (m_h) {
        abemu_free(m_h);
        m_h = nullptr;
    }
}

bool EmulatorCore::loadFile(const QString &path) {
    const QByteArray p = path.toUtf8();
    const int rc = abemu_load_file(m_h, p.constData());
    m_loaded = (rc == 0);
    return m_loaded;
}

QString EmulatorCore::lastError() const {
    const char *e = abemu_last_error(m_h);
    return QString::fromUtf8(e ? e : "");
}

void EmulatorCore::reset() { abemu_reset(m_h); }
void EmulatorCore::runFrame() { abemu_run_frame(m_h); }

void EmulatorCore::setButton(Button b, bool pressed) {
    abemu_set_button(m_h, static_cast<int>(b), pressed ? 1 : 0);
}

const QImage &EmulatorCore::frame() {
    const unsigned char *fb = abemu_framebuffer(m_h);
    if (fb) {
        std::memcpy(m_image.bits(), fb,
                    static_cast<size_t>(screenWidth()) * screenHeight() * 4);
    }
    return m_image;
}

int EmulatorCore::renderAudio(QVector<float> &out, unsigned int sampleRate,
                              float volume) {
    // Generous headroom: at 60fps/44.1kHz a frame is ~735 pairs.
    const int maxPairs = 4096;
    if (out.size() < maxPairs * 2)
        out.resize(maxPairs * 2);
    return abemu_render_audio(m_h, out.data(), maxPairs, sampleRate, volume);
}

void EmulatorCore::ledRgb(int &r, int &g, int &b) const {
    unsigned char rr = 0, gg = 0, bb = 0;
    abemu_led_rgb(m_h, &rr, &gg, &bb);
    r = rr;
    g = gg;
    b = bb;
}

bool EmulatorCore::ledTx() const { return abemu_led_tx(m_h) != 0; }
bool EmulatorCore::ledRx() const { return abemu_led_rx(m_h) != 0; }

QString EmulatorCore::takeSerial() {
    unsigned char buf[512];
    const int n = abemu_take_serial(m_h, buf, sizeof(buf));
    if (n <= 0)
        return QString();
    return QString::fromUtf8(reinterpret_cast<const char *>(buf), n);
}

bool EmulatorCore::eepromDirty() const { return abemu_eeprom_dirty(m_h) != 0; }
bool EmulatorCore::saveEeprom() { return abemu_save_eeprom(m_h) == 0; }
bool EmulatorCore::saveState() { return abemu_save_state(m_h) == 0; }
bool EmulatorCore::loadState() { return abemu_load_state(m_h) == 0; }

bool EmulatorCore::screenshotPng(const QString &path) {
    const QByteArray p = path.toUtf8();
    return abemu_screenshot_png(m_h, p.constData()) == 0;
}

bool EmulatorCore::gifStart(const QString &path) {
    const QByteArray p = path.toUtf8();
    return abemu_gif_start(m_h, p.constData()) == 0;
}

bool EmulatorCore::gifStop() { return abemu_gif_stop(m_h) == 0; }
bool EmulatorCore::gifRecording() const { return abemu_gif_recording(m_h) != 0; }

QString EmulatorCore::dumpRegs() {
    const int n = abemu_dump_regs(m_h, m_txt.data(), m_txt.size());
    return QString::fromUtf8(m_txt.constData(), n);
}

int EmulatorCore::cpuType() const { return abemu_cpu_type(m_h); }

QString EmulatorCore::title() const {
    char buf[128];
    const int n = abemu_title(const_cast<Emu *>(m_h), buf, sizeof(buf));
    return QString::fromUtf8(buf, n);
}

int EmulatorCore::screenWidth() { return abemu_screen_width(); }
int EmulatorCore::screenHeight() { return abemu_screen_height(); }
