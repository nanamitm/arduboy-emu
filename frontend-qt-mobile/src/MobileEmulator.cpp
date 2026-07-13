#include "MobileEmulator.h"

#include <QDir>
#include <QFile>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QPainter>
#include <QSaveFile>
#include <QStandardPaths>
#include <QTimer>
#include <QUuid>
#include <QUrl>

namespace {
constexpr int kFrameIntervalMs = 16;
constexpr float kAudioVolume = 0.15f;
constexpr qint64 kMaximumRemoteRomBytes = 2 * 1024 * 1024;
}

MobileEmulator::MobileEmulator(QQuickItem *parent) : QQuickPaintedItem(parent), m_audio(this) {
    setAntialiasing(false);
    setRenderTarget(QQuickPaintedItem::FramebufferObject);
    m_audio.start(44100);
    m_network = new QNetworkAccessManager(this);
    m_timer = new QTimer(this);
    m_timer->setTimerType(Qt::PreciseTimer);
    connect(m_timer, &QTimer::timeout, this, &MobileEmulator::tick);
    m_timer->start(kFrameIntervalMs);
}

MobileEmulator::~MobileEmulator() {
    if (m_core.isLoaded() && m_core.eepromDirty())
        m_core.saveEeprom();
}

void MobileEmulator::paint(QPainter *painter) {
    painter->fillRect(boundingRect(), Qt::black);
    const QImage &frame = m_core.frame();
    painter->setRenderHint(QPainter::SmoothPixmapTransform, false);
    painter->drawImage(boundingRect(), frame);
}

void MobileEmulator::loadRom(const QUrl &url) {
    if (!url.isLocalFile()) {
        setStatus(QStringLiteral("Choose a local .hex, .arduboy, or .elf file"));
        return;
    }
    if (m_core.isLoaded() && m_core.eepromDirty())
        m_core.saveEeprom();
    if (!m_core.loadFile(url.toLocalFile())) {
        setStatus(QStringLiteral("Load failed: %1").arg(m_core.lastError()));
        return;
    }
    setStatus(QStringLiteral("Loaded %1").arg(m_core.title()));
    emit loadedChanged();
    update();
}

void MobileEmulator::loadRemoteRom(const QUrl &url) {
    QUrl downloadUrl = url;
    if (downloadUrl.scheme().compare(QStringLiteral("arduboy"), Qt::CaseInsensitive) == 0)
        downloadUrl.setScheme(QStringLiteral("https"));

    if (!downloadUrl.isValid() || downloadUrl.scheme() != QStringLiteral("https") ||
        downloadUrl.host().isEmpty()) {
        setStatus(QStringLiteral("The ROM link must use arduboy:// or https://"));
        return;
    }

    if (m_downloadReply) {
        m_downloadReply->abort();
        m_downloadReply->deleteLater();
    }

    setStatus(QStringLiteral("Downloading ROM…"));
    QNetworkRequest request(downloadUrl);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::NoLessSafeRedirectPolicy);
    m_downloadReply = m_network->get(request);
    QNetworkReply *reply = m_downloadReply;

    connect(reply, &QNetworkReply::downloadProgress, this,
            [reply](qint64 received, qint64 total) {
                if (received > kMaximumRemoteRomBytes || total > kMaximumRemoteRomBytes)
                    reply->abort();
            });
    connect(reply, &QNetworkReply::finished, this, [this, reply] {
        if (reply != m_downloadReply) {
            reply->deleteLater();
            return;
        }
        m_downloadReply = nullptr;

        if (reply->error() != QNetworkReply::NoError) {
            setStatus(QStringLiteral("Download failed: %1").arg(reply->errorString()));
            reply->deleteLater();
            return;
        }

        const QByteArray rom = reply->readAll();
        reply->deleteLater();
        if (rom.isEmpty() || rom.size() > kMaximumRemoteRomBytes) {
            setStatus(QStringLiteral("Downloaded ROM is empty or too large"));
            return;
        }

        const QString cacheDir = QStandardPaths::writableLocation(QStandardPaths::CacheLocation)
            + QStringLiteral("/roms");
        if (!QDir().mkpath(cacheDir)) {
            setStatus(QStringLiteral("Unable to create the ROM cache"));
            return;
        }
        const QString path = cacheDir + QStringLiteral("/remote-")
            + QUuid::createUuid().toString(QUuid::WithoutBraces) + QStringLiteral(".hex");
        QSaveFile file(path);
        if (!file.open(QIODevice::WriteOnly) || file.write(rom) != rom.size() || !file.commit()) {
            setStatus(QStringLiteral("Unable to save downloaded ROM"));
            return;
        }
        if (!m_downloadedRomPath.isEmpty())
            QFile::remove(m_downloadedRomPath);
        m_downloadedRomPath = path;
        loadRom(QUrl::fromLocalFile(path));
    });
}

void MobileEmulator::setButton(int button, bool pressed) {
    if (button < EmulatorCore::Up || button > EmulatorCore::B)
        return;
    m_core.setButton(static_cast<EmulatorCore::Button>(button), pressed);
}

void MobileEmulator::reset() {
    if (m_core.isLoaded())
        m_core.reset();
}

void MobileEmulator::setStatus(const QString &status) {
    if (m_status == status)
        return;
    m_status = status;
    emit statusChanged();
}

void MobileEmulator::tick() {
    if (!m_core.isLoaded())
        return;
    m_core.runFrame();
    const int pairs = m_core.renderAudio(m_audioSamples, m_audio.sampleRate(), kAudioVolume);
    if (pairs > 0)
        m_audio.writeSamples(m_audioSamples.constData(), pairs);
    update();
}
