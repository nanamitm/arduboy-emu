#pragma once

#include <QQuickPaintedItem>
#include <QString>
#include <QUrl>
#include <QVector>

#include "AudioOutput.h"
#include "EmulatorCore.h"

class QTimer;
class QPainter;
class QNetworkAccessManager;
class QNetworkReply;

// Touch-first Qt Quick surface.  EmulatorCore remains the single C++ wrapper
// around the shared Rust ABI used by both desktop and Android clients.
class MobileEmulator : public QQuickPaintedItem {
    Q_OBJECT
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)
    Q_PROPERTY(bool loaded READ loaded NOTIFY loadedChanged)

public:
    explicit MobileEmulator(QQuickItem *parent = nullptr);
    ~MobileEmulator() override;

    void paint(QPainter *painter) override;
    QString status() const { return m_status; }
    bool loaded() const { return m_core.isLoaded(); }

    Q_INVOKABLE void loadRom(const QUrl &url);
    Q_INVOKABLE void setButton(int button, bool pressed);
    Q_INVOKABLE void reset();

public slots:
    // Handles the arduboy:// URL emitted by ProjectABE's QR workflow.
    void loadRemoteRom(const QUrl &url);

signals:
    void statusChanged();
    void loadedChanged();

private:
    void setStatus(const QString &status);
    void tick();

    EmulatorCore m_core;
    AudioOutput m_audio;
    QNetworkAccessManager *m_network = nullptr;
    QNetworkReply *m_downloadReply = nullptr;
    QString m_downloadedRomPath;
    QTimer *m_timer = nullptr;
    QVector<float> m_audioSamples;
    QString m_status = QStringLiteral("Open a ROM to start");
};
