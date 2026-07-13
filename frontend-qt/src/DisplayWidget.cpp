#include "DisplayWidget.h"

#include <QColor>
#include <QFont>
#include <QLinearGradient>
#include <QPainter>
#include <QSizeF>

namespace {

struct SkinSpec {
    QSizeF size;
    QRectF screen;
    QPointF dpad;
    QPointF actions;
    qreal dpadButton;
    qreal actionButton;
    QColor faceA;
    QColor faceB;
    QColor edge;
    QColor screenFrame;
    QString title;
    bool terminal = false;
};

SkinSpec specFor(DisplayWidget::Skin skin) {
    switch (skin) {
    case DisplayWidget::Skin::Microcard:
        return {{430, 318}, {95, 57, 241, 120}, {17, 200}, {330, 207}, 28, 44,
                QColor("#467a86"), QColor("#102832"), QColor("#70aeb6"), QColor("#071014"),
                QStringLiteral("MICROCARD")};
    case DisplayWidget::Skin::Tama:
        return {{300, 357}, {63, 107, 174, 87}, {24, 221}, {208, 236}, 31, 42,
                QColor("#fcf6ce"), QColor("#b87a53"), QColor("#fff0ae"), QColor("#5d4a37"),
                QStringLiteral("TAMA")};
    case DisplayWidget::Skin::Pipboy:
        return {{540, 367}, {178, 88, 221, 110}, {43, 230}, {432, 242}, 34, 48,
                QColor("#5c6656"), QColor("#11170f"), QColor("#849174"), QColor("#10180c"),
                QStringLiteral("PIP-BOY 3000"), true};
    case DisplayWidget::Skin::PipboyMkIv:
        return {{480, 364}, {106, 95, 168, 84}, {230, 204}, {370, 240}, 31, 46,
                QColor("#757a71"), QColor("#151914"), QColor("#a7aa9b"), QColor("#12170f"),
                QStringLiteral("PIP-BOY 3000 MARK IV"), true};
    case DisplayWidget::Skin::Arduboy:
    default:
        return {{320, 512}, {32, 92, 256, 128}, {22, 270}, {220, 292}, 32, 46,
                QColor("#343c47"), QColor("#11161c"), QColor("#586371"), QColor("#0a0d10"),
                QStringLiteral("ARDUBOY")};
    }
}

void drawPad(QPainter &p, const QRectF &rect, const QString &label, bool round = false) {
    p.setPen(QPen(QColor("#303b49"), 1.0));
    p.setBrush(QColor("#1b2531"));
    if (round)
        p.drawEllipse(rect);
    else
        p.drawRoundedRect(rect, 7, 7);
    p.setPen(QColor("#f0f5f7"));
    QFont font = p.font();
    font.setBold(true);
    font.setPointSizeF(round ? rect.height() * .34 : rect.height() * .42);
    p.setFont(font);
    p.drawText(rect, Qt::AlignCenter, label);
}

} // namespace

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

void DisplayWidget::setSkin(Skin skin) {
    if (m_skin == skin)
        return;
    m_skin = skin;
    updateGeometry();
    update();
}

QSize DisplayWidget::scaledSize(int screenScale) const {
    const SkinSpec spec = specFor(m_skin);
    const qreal factor = (128.0 * qBound(1, screenScale, 6)) / spec.screen.width();
    return QSizeF(spec.size.width() * factor, spec.size.height() * factor).toSize();
}

QSize DisplayWidget::sizeHint() const {
    return scaledSize(4);
}

void DisplayWidget::paintEvent(QPaintEvent *) {
    QPainter p(this);
    p.fillRect(rect(), QColor("#0b0e12"));
    if (m_frame.isNull())
        return;

    const SkinSpec spec = specFor(m_skin);
    const qreal scale = qMin(width() / spec.size.width(), height() / spec.size.height());
    const qreal offsetX = (width() - spec.size.width() * scale) / 2.0;
    const qreal offsetY = (height() - spec.size.height() * scale) / 2.0;
    p.translate(offsetX, offsetY);
    p.scale(scale, scale);

    const QRectF deviceRect(QPointF(0, 0), spec.size);
    QLinearGradient face(deviceRect.topLeft(), deviceRect.bottomRight());
    face.setColorAt(0, spec.faceA);
    face.setColorAt(.62, spec.faceB);
    face.setColorAt(1, spec.faceB.darker(130));
    p.setPen(QPen(spec.edge, 2.0));
    p.setBrush(face);
    const qreal radius = m_skin == Skin::Arduboy ? 34 : 28;
    p.drawRoundedRect(deviceRect.adjusted(1, 1, -1, -1), radius, radius);

    p.setPen(spec.terminal ? QColor("#c5d2a9") : QColor("#d6dde5"));
    QFont brand = p.font();
    brand.setBold(true);
    brand.setLetterSpacing(QFont::AbsoluteSpacing, 3.0);
    brand.setPointSizeF(spec.terminal ? 12 : 11);
    p.setFont(brand);
    p.drawText(QRectF(0, 24, spec.size.width(), 22), Qt::AlignCenter, spec.title);

    p.setPen(Qt::NoPen);
    p.setBrush(QColor("#d26b57"));
    p.drawEllipse(QPointF(24, 30), 2.0, 2.0);
    p.setBrush(QColor("#e7b94f"));
    p.drawEllipse(QPointF(32, 30), 2.0, 2.0);

    p.setPen(QPen(QColor("#050708"), 2.0));
    p.setBrush(spec.screenFrame);
    p.drawRoundedRect(spec.screen.adjusted(-6, -6, 6, 6), 4, 4);
    const QRectF imageRect = spec.screen.adjusted(1, 1, -1, -1);
    p.setRenderHint(QPainter::SmoothPixmapTransform, m_smooth);
    p.drawImage(imageRect, m_frame);

    const qreal step = spec.dpadButton + 3;
    drawPad(p, {spec.dpad.x() + step, spec.dpad.y(), spec.dpadButton, spec.dpadButton}, QStringLiteral("▲"));
    drawPad(p, {spec.dpad.x(), spec.dpad.y() + step, spec.dpadButton, spec.dpadButton}, QStringLiteral("◀"));
    drawPad(p, {spec.dpad.x() + step * 2, spec.dpad.y() + step, spec.dpadButton, spec.dpadButton}, QStringLiteral("▶"));
    drawPad(p, {spec.dpad.x() + step, spec.dpad.y() + step * 2, spec.dpadButton, spec.dpadButton}, QStringLiteral("▼"));

    const qreal actionGap = 8;
    QRectF bRect(spec.actions, QSizeF(spec.actionButton, spec.actionButton));
    QRectF aRect(spec.actions + QPointF(spec.actionButton + actionGap, 0), QSizeF(spec.actionButton, spec.actionButton));
    drawPad(p, bRect, QStringLiteral("B"), true);
    drawPad(p, aRect, QStringLiteral("A"), true);
    if (spec.terminal) {
        p.setPen(QPen(QColor("#758060"), 2.0));
        p.setBrush(Qt::NoBrush);
        p.drawEllipse(QRectF(spec.size.width() - 94, 54, 46, 30));
    }
}
