#include "DisplayWidget.h"

#include <QPainter>

DisplayWidget::DisplayWidget(QWidget *parent) : QWidget(parent) {
    setMinimumSize(128, 64);
    // Black background so letterbox bars render as an unlit bezel.
    QPalette pal = palette();
    pal.setColor(QPalette::Window, Qt::black);
    setPalette(pal);
    setAutoFillBackground(true);
    m_frame = QImage(128, 64, QImage::Format_RGBA8888);
    m_frame.fill(Qt::black);
}

void DisplayWidget::setFrame(const QImage &img) {
    m_frame = img;
    update();
}

void DisplayWidget::setSmooth(bool smooth) {
    m_smooth = smooth;
    update();
}

QSize DisplayWidget::sizeHint() const {
    return QSize(128 * 4, 64 * 4);
}

void DisplayWidget::paintEvent(QPaintEvent *) {
    QPainter p(this);
    p.fillRect(rect(), Qt::black);
    if (m_frame.isNull())
        return;

    // Largest integer-friendly rect that fits while keeping the 2:1 aspect.
    const double srcAspect =
        static_cast<double>(m_frame.width()) / m_frame.height();
    int w = width();
    int h = static_cast<int>(w / srcAspect);
    if (h > height()) {
        h = height();
        w = static_cast<int>(h * srcAspect);
    }
    const int x = (width() - w) / 2;
    const int y = (height() - h) / 2;

    p.setRenderHint(QPainter::SmoothPixmapTransform, m_smooth);
    p.drawImage(QRect(x, y, w, h), m_frame);
}
