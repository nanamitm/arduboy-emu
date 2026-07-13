// Qt6 GUI client entry point for the arduboy-core emulator.
#include <QApplication>

#include "MainWindow.h"

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setApplicationName("Arduboy Emulator (Qt)");
    QApplication::setOrganizationName("arduboy-emu");

    MainWindow win;
    win.show();

    // Optional ROM path as the first positional argument.
    const QStringList args = QApplication::arguments();
    if (args.size() > 1)
        win.openPath(args.at(1));

    return app.exec();
}
