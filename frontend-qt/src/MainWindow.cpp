#include "MainWindow.h"

#include "AudioOutput.h"
#include "DisplayWidget.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCloseEvent>
#include <QDateTime>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QMimeData>
#include <QUrl>
#include <QDockWidget>
#include <QFileDialog>
#include <QFileInfo>
#include <QFont>
#include <QKeyEvent>
#include <QLabel>
#include <QMenuBar>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QSettings>
#include <QStatusBar>
#include <QTimer>

namespace {
constexpr float kAudioVolume = 0.15f; // matches the desktop frontend
constexpr int kFrameIntervalMs = 16;  // ~60 fps
} // namespace

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent) {
    setWindowTitle("Arduboy Emulator (Qt)");
    setAcceptDrops(true); // drag a .hex/.arduboy/.elf onto the window to load it

    m_display = new DisplayWidget(this);
    QSettings settings;
    const int savedSkin = settings.value(QStringLiteral("view/skin"), 0).toInt();
    if (savedSkin >= 0 && savedSkin <= static_cast<int>(DisplayWidget::Skin::PipboyMkIv))
        m_display->setSkin(static_cast<DisplayWidget::Skin>(savedSkin));
    setCentralWidget(m_display);
    connect(m_display, &DisplayWidget::buttonChanged, this, [this](int button, bool pressed) {
        if (button >= EmulatorCore::Up && button <= EmulatorCore::B)
            setButtonSource(static_cast<EmulatorCore::Button>(button), pressed, InputSource::Skin);
    });

    m_audio = new AudioOutput(this);
    m_audio->start(44100);

    buildMenus();

    // Status bar
    m_cpuLabel = new QLabel(this);
    m_ledLabel = new QLabel(this);
    m_stateLabel = new QLabel(this);
    m_fpsLabel = new QLabel(this);
    statusBar()->addWidget(m_stateLabel, 1);
    statusBar()->addPermanentWidget(m_ledLabel);
    statusBar()->addPermanentWidget(m_cpuLabel);
    statusBar()->addPermanentWidget(m_fpsLabel);

    // Register dock (hidden by default)
    m_regDock = new QDockWidget(tr("Registers"), this);
    m_regView = new QPlainTextEdit(m_regDock);
    m_regView->setReadOnly(true);
    QFont mono("Consolas");
    mono.setStyleHint(QFont::Monospace);
    m_regView->setFont(mono);
    m_regDock->setWidget(m_regView);
    addDockWidget(Qt::RightDockWidgetArea, m_regDock);
    m_regDock->hide();

    m_timer = new QTimer(this);
    m_timer->setTimerType(Qt::PreciseTimer);
    connect(m_timer, &QTimer::timeout, this, &MainWindow::tick);
    m_timer->start(kFrameIntervalMs);

    m_fpsClock.start();
    setScale(m_scale);
    updateStatus();
}

MainWindow::~MainWindow() = default;

// ─── Menu construction ──────────────────────────────────────────────────────

void MainWindow::buildMenus() {
    QMenu *file = menuBar()->addMenu(tr("&File"));
    file->addAction(tr("&Open ROM..."), QKeySequence::Open, this, &MainWindow::openRom);
    file->addAction(tr("&Reload"), QKeySequence(Qt::Key_R), this, &MainWindow::reloadRom);
    file->addSeparator();
    file->addAction(tr("&Screenshot (PNG)"), QKeySequence(Qt::Key_S), this,
                    &MainWindow::takeScreenshot);
    file->addSeparator();
    file->addAction(tr("Save State"), QKeySequence(Qt::Key_F5), this,
                    &MainWindow::saveState);
    file->addAction(tr("Load State"), QKeySequence(Qt::Key_F9), this,
                    &MainWindow::loadState);
    file->addSeparator();
    file->addAction(tr("E&xit"), QKeySequence(Qt::CTRL | Qt::Key_Q), this,
                    &QWidget::close);

    QMenu *emu = menuBar()->addMenu(tr("&Emulation"));
    emu->addAction(tr("&Pause / Resume"), QKeySequence(Qt::Key_P), this,
                   &MainWindow::togglePause);
    emu->addAction(tr("Rese&t"), QKeySequence(Qt::CTRL | Qt::Key_R), this,
                   &MainWindow::reset);

    QMenu *view = menuBar()->addMenu(tr("&View"));
    for (int s = 1; s <= 6; ++s) {
        view->addAction(tr("Scale %1x").arg(s), QKeySequence(Qt::Key_0 + s),
                        this, [this, s]() { setScale(s); });
    }
    view->addSeparator();
    QMenu *skins = view->addMenu(tr("&Skin"));
    QActionGroup *skinGroup = new QActionGroup(this);
    skinGroup->setExclusive(true);
    const QStringList skinNames = {tr("Arduboy"), tr("Microcard"), tr("Tama"),
                                   tr("Pip-Boy 3000"), tr("Pip-Boy Mk IV")};
    for (int i = 0; i < skinNames.size(); ++i) {
        QAction *skin = skins->addAction(skinNames.at(i));
        skin->setCheckable(true);
        skin->setChecked(i == static_cast<int>(m_display->skin()));
        skinGroup->addAction(skin);
        connect(skin, &QAction::triggered, this, [this, i]() { setSkin(i); });
    }
    view->addAction(tr("&Fullscreen"), QKeySequence(Qt::Key_F11), this,
                    &MainWindow::toggleFullscreen);
    QAction *smooth = view->addAction(tr("&Smooth scaling"));
    smooth->setCheckable(true);
    connect(smooth, &QAction::triggered, this, &MainWindow::toggleSmooth);
    QAction *regs = view->addAction(tr("&Registers"));
    regs->setCheckable(true);
    connect(regs, &QAction::triggered, this, &MainWindow::toggleRegisters);

    QMenu *audio = menuBar()->addMenu(tr("&Audio"));
    QAction *mute = audio->addAction(tr("&Mute"), QKeySequence(Qt::Key_M), this,
                                     &MainWindow::toggleMute);
    mute->setCheckable(true);

    QMenu *rec = menuBar()->addMenu(tr("&Record"));
    rec->addAction(tr("&GIF Start/Stop"), QKeySequence(Qt::Key_G), this,
                   &MainWindow::toggleGif);

    QMenu *help = menuBar()->addMenu(tr("&Help"));
    help->addAction(tr("&About"), this, &MainWindow::about);
}

// ─── Emulation loop ─────────────────────────────────────────────────────────

void MainWindow::tick() {
    pollGamepad();
    if (m_paused || !m_core.isLoaded())
        return;

    m_core.runFrame();

    // Audio: render this frame's samples and push to the sink.
    const int pairs = m_core.renderAudio(m_audioBuf, m_audio->sampleRate(), kAudioVolume);
    m_audio->writeSamples(m_audioBuf.constData(), pairs);

    // Video
    m_display->setFrame(m_core.frame());

    // FPS accounting
    ++m_frameCounter;
    if (m_fpsClock.elapsed() >= 500) {
        m_fps = m_frameCounter * 1000.0 / m_fpsClock.elapsed();
        m_frameCounter = 0;
        m_fpsClock.restart();
        updateStatus();
        if (m_regDock->isVisible())
            m_regView->setPlainText(m_core.dumpRegs());
    }
}

// ─── File / state actions ───────────────────────────────────────────────────

void MainWindow::openRom() {
    const QString path = QFileDialog::getOpenFileName(
        this, tr("Open ROM"), QString(),
        tr("Arduboy ROMs (*.hex *.arduboy *.elf);;All files (*)"));
    if (!path.isEmpty())
        openPath(path);
}

void MainWindow::openPath(const QString &path) {
    if (!m_core.loadFile(path)) {
        QMessageBox::warning(this, tr("Load failed"),
                             tr("Could not load %1:\n%2").arg(path, m_core.lastError()));
        return;
    }
    m_romPath = path;
    const QString title = m_core.title();
    const QString base = QFileInfo(path).fileName();
    setWindowTitle(title.isEmpty()
                       ? tr("Arduboy Emulator (Qt) — %1").arg(base)
                       : tr("Arduboy Emulator (Qt) — %1 [%2]").arg(title, base));
    setPaused(false);
    updateStatus();
}

void MainWindow::reloadRom() {
    if (!m_romPath.isEmpty())
        openPath(m_romPath);
}

void MainWindow::reset() {
    if (!m_core.isLoaded())
        return;
    m_core.reset();
    // Resetting while paused resumes emulation, so the fresh boot is visible.
    if (m_paused)
        setPaused(false);
}

void MainWindow::takeScreenshot() {
    if (!m_core.isLoaded())
        return;
    const QString base = m_romPath.isEmpty() ? QStringLiteral("screenshot")
                                             : QFileInfo(m_romPath).completeBaseName();
    const QString name = QStringLiteral("%1-%2.png")
                             .arg(base)
                             .arg(QDateTime::currentDateTime().toString("yyyyMMdd-hhmmss"));
    if (m_core.screenshotPng(name))
        statusBar()->showMessage(tr("Saved %1").arg(name), 2000);
    else
        statusBar()->showMessage(tr("Screenshot failed: %1").arg(m_core.lastError()), 3000);
}

void MainWindow::toggleGif() {
    if (!m_core.isLoaded())
        return;
    if (m_core.gifRecording()) {
        if (m_core.gifStop())
            statusBar()->showMessage(tr("GIF saved"), 2000);
        else
            statusBar()->showMessage(tr("GIF save failed: %1").arg(m_core.lastError()), 3000);
    } else {
        const QString base = m_romPath.isEmpty() ? QStringLiteral("recording")
                                                 : QFileInfo(m_romPath).completeBaseName();
        const QString name = QStringLiteral("%1-%2.gif")
                                 .arg(base)
                                 .arg(QDateTime::currentDateTime().toString("yyyyMMdd-hhmmss"));
        if (m_core.gifStart(name))
            statusBar()->showMessage(tr("Recording GIF to %1...").arg(name), 2000);
    }
    updateStatus();
}

void MainWindow::saveState() {
    if (m_core.isLoaded())
        statusBar()->showMessage(m_core.saveState() ? tr("State saved")
                                                     : tr("Save failed: %1").arg(m_core.lastError()),
                                 2000);
}

void MainWindow::loadState() {
    if (m_core.isLoaded())
        statusBar()->showMessage(m_core.loadState() ? tr("State loaded")
                                                     : tr("Load failed: %1").arg(m_core.lastError()),
                                 2000);
}

// ─── Toggles ────────────────────────────────────────────────────────────────

void MainWindow::togglePause() { setPaused(!m_paused); }

void MainWindow::setPaused(bool paused) {
    m_paused = paused;
    updateStatus();
}

void MainWindow::toggleMute() {
    m_audio->setMuted(!m_audio->muted());
    updateStatus();
}

void MainWindow::toggleSmooth() { m_display->setSmooth(!m_display->smooth()); }

void MainWindow::toggleRegisters() {
    m_regDock->setVisible(!m_regDock->isVisible());
    if (m_regDock->isVisible() && m_core.isLoaded())
        m_regView->setPlainText(m_core.dumpRegs());
}

void MainWindow::toggleFullscreen() {
    if (isFullScreen())
        showNormal();
    else
        showFullScreen();
}

void MainWindow::setScale(int factor) {
    m_scale = qBound(1, factor, 6);
    if (isFullScreen())
        return;
    // Size the whole device skin so its display area remains at the selected scale.
    const QSize size = m_display->scaledSize(m_scale);
    m_display->setMinimumSize(size);
    resize(size.width(), size.height() + menuBar()->height() + statusBar()->height());
    m_display->setMinimumSize(EmulatorCore::screenWidth(),
                              EmulatorCore::screenHeight());
}

void MainWindow::setSkin(int skin) {
    if (skin < 0 || skin > static_cast<int>(DisplayWidget::Skin::PipboyMkIv))
        return;
    m_display->setSkin(static_cast<DisplayWidget::Skin>(skin));
    QSettings().setValue(QStringLiteral("view/skin"), skin);
    setScale(m_scale);
}

// ─── Input ──────────────────────────────────────────────────────────────────

bool MainWindow::mapButton(int key, EmulatorCore::Button &out) const {
    switch (key) {
    case Qt::Key_Up: out = EmulatorCore::Up; return true;
    case Qt::Key_Down: out = EmulatorCore::Down; return true;
    case Qt::Key_Left: out = EmulatorCore::Left; return true;
    case Qt::Key_Right: out = EmulatorCore::Right; return true;
    case Qt::Key_Z: out = EmulatorCore::A; return true;
    case Qt::Key_X: out = EmulatorCore::B; return true;
    default: return false;
    }
}

void MainWindow::setButtonSource(EmulatorCore::Button button, bool pressed, InputSource sourceKind) {
    const int index = static_cast<int>(button);
    if (index < EmulatorCore::Up || index > EmulatorCore::B)
        return;
    auto &source = sourceKind == InputSource::Keyboard ? m_keyboardButtons
                   : sourceKind == InputSource::Skin ? m_skinButtons : m_gamepadButtons;
    if (source.at(index) == pressed)
        return;
    const bool wasPressed = m_keyboardButtons.at(index) || m_skinButtons.at(index) || m_gamepadButtons.at(index);
    source.at(index) = pressed;
    const bool isPressed = m_keyboardButtons.at(index) || m_skinButtons.at(index) || m_gamepadButtons.at(index);
    if (wasPressed != isPressed)
        m_core.setButton(button, isPressed);
}

void MainWindow::pollGamepad() {
    const GamepadInput::Snapshot state = m_gamepad.poll();
    for (int i = EmulatorCore::Up; i <= EmulatorCore::B; ++i)
        setButtonSource(static_cast<EmulatorCore::Button>(i), state.buttons.at(i), InputSource::Gamepad);
    if (m_gamepadConnected != state.connected) {
        m_gamepadConnected = state.connected;
        updateStatus();
    }
}

void MainWindow::keyPressEvent(QKeyEvent *event) {
    EmulatorCore::Button b;
    if (!event->isAutoRepeat() && mapButton(event->key(), b)) {
        setButtonSource(b, true, InputSource::Keyboard);
        event->accept();
        return;
    }
    if (event->key() == Qt::Key_Escape && isFullScreen()) {
        showNormal();
        event->accept();
        return;
    }
    QMainWindow::keyPressEvent(event);
}

void MainWindow::keyReleaseEvent(QKeyEvent *event) {
    EmulatorCore::Button b;
    if (!event->isAutoRepeat() && mapButton(event->key(), b)) {
        setButtonSource(b, false, InputSource::Keyboard);
        event->accept();
        return;
    }
    QMainWindow::keyReleaseEvent(event);
}

bool MainWindow::isSupportedRom(const QString &path) {
    const QString p = path.toLower();
    return p.endsWith(".hex") || p.endsWith(".arduboy") || p.endsWith(".elf");
}

void MainWindow::dragEnterEvent(QDragEnterEvent *event) {
    if (!event->mimeData()->hasUrls())
        return;
    for (const QUrl &url : event->mimeData()->urls()) {
        if (isSupportedRom(url.toLocalFile())) {
            event->acceptProposedAction(); // show the "copy" cursor
            return;
        }
    }
}

void MainWindow::dropEvent(QDropEvent *event) {
    for (const QUrl &url : event->mimeData()->urls()) {
        const QString path = url.toLocalFile();
        if (isSupportedRom(path)) {
            openPath(path); // loads the first supported ROM in the drop
            event->acceptProposedAction();
            return;
        }
    }
}

void MainWindow::closeEvent(QCloseEvent *event) {
    if (m_core.isLoaded() && m_core.gifRecording())
        m_core.gifStop();
    if (m_core.isLoaded() && m_core.eepromDirty())
        m_core.saveEeprom();
    event->accept();
}

// ─── Status bar ─────────────────────────────────────────────────────────────

void MainWindow::updateStatus() {
    if (!m_core.isLoaded()) {
        m_stateLabel->setText(tr("No ROM loaded — File ▸ Open ROM, or drag a .hex here"));
        m_cpuLabel->clear();
        m_ledLabel->clear();
        m_fpsLabel->clear();
        return;
    }

    QStringList flags;
    if (m_paused) flags << tr("PAUSED");
    if (m_audio->muted()) flags << tr("MUTE");
    if (m_core.gifRecording()) flags << tr("● REC");
    if (m_gamepadConnected) flags << tr("GAMEPAD");
    m_stateLabel->setText(flags.join("  "));

    m_cpuLabel->setText(m_core.cpuType() == 1 ? "ATmega328P" : "ATmega32u4");

    int r, g, b;
    m_core.ledRgb(r, g, b);
    m_ledLabel->setText(QStringLiteral("RGB(%1,%2,%3) %4%5")
                            .arg(r)
                            .arg(g)
                            .arg(b)
                            .arg(m_core.ledTx() ? "TX " : "")
                            .arg(m_core.ledRx() ? "RX" : ""));

    m_fpsLabel->setText(QStringLiteral("%1 fps").arg(m_fps, 0, 'f', 1));
}

void MainWindow::about() {
    QMessageBox::about(
        this, tr("About"),
        tr("<h3>Arduboy Emulator — Qt frontend</h3>"
           "<p>A Qt6/C++ GUI client for the arduboy-core (Rust) emulator, "
           "linked via the arduboy_ffi C ABI.</p>"
           "<p><b>Controls:</b> Arrows = D-pad, Z = A, X = B.<br>"
           "R = reload, S = screenshot, G = GIF, P = pause, M = mute,<br>"
           "F5/F9 = save/load state, 1–6 = scale, F11 = fullscreen.</p>"));
}
