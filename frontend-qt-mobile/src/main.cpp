#include <QDesktopServices>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QTimer>

#ifdef Q_OS_ANDROID
#include <QJniObject>
#endif

#include "MobileEmulator.h"

int main(int argc, char *argv[]) {
#ifdef Q_OS_ANDROID
    // The Android OpenSSL package is bundled as libcrypto_3.so/libssl_3.so.
    // Qt's TLS plugin must be told to use that suffix before it initializes.
    qputenv("ANDROID_OPENSSL_SUFFIX", "_3");
#endif
    QGuiApplication app(argc, argv);
    QGuiApplication::setApplicationName(QStringLiteral("Arduboy Emulator"));
    QGuiApplication::setOrganizationName(QStringLiteral("arduboy-emu"));
    QQuickStyle::setStyle(QStringLiteral("Material"));

    qmlRegisterType<MobileEmulator>("ArduboyMobile", 1, 0, "MobileEmulator");
    QQmlApplicationEngine engine;
    engine.loadFromModule("ArduboyMobile", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;

    auto *emulator = engine.rootObjects().constFirst()->findChild<MobileEmulator *>("emulator");
    if (emulator)
        QDesktopServices::setUrlHandler(QStringLiteral("arduboy"), emulator,
                                        SLOT(loadRemoteRom(QUrl)));

#ifdef Q_OS_ANDROID
    // The Java activity retains VIEW intents from both cold starts and
    // onNewIntent(), then this timer delivers each URL to the QML item.
    auto *romIntentPoller = new QTimer(&app);
    romIntentPoller->setInterval(250);
    QObject::connect(romIntentPoller, &QTimer::timeout, &app, [emulator] {
        if (!emulator)
            return;
        const QJniObject value = QJniObject::callStaticObjectMethod(
            "io/github/nanamitm/arduboyemu/ArduboyActivity",
            "takePendingRomUrl", "()Ljava/lang/String;");
        if (value.isValid()) {
            const QString url = value.toString();
            if (!url.isEmpty())
                emulator->loadRemoteRom(QUrl(url));
        }
    });
    romIntentPoller->start();
#endif
    return app.exec();
}
