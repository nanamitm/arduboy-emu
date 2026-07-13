// DisplayWidget — paints the 128x64 emulator framebuffer, scaled to fit while
// preserving the 2:1 aspect ratio, with letterboxing. Nearest-neighbour scaling
// keeps the pixel-art crisp.
#pragma once

#include <QImage>
#include <QPointF>
#include <QWidget>

class DisplayWidget : public QWidget {
    Q_OBJECT
public:
    enum class Skin {
        Arduboy,
        Microcard,
        Tama,
        Pipboy,
        PipboyMkIv,
    };

    explicit DisplayWidget(QWidget *parent = nullptr);

    // Update the displayed image (a shallow copy; the pixels are memcpy'd by the
    // caller each frame). Triggers a repaint.
    void setFrame(const QImage &img);

    // Smooth (bilinear) vs. crisp (nearest) scaling.
    void setSmooth(bool smooth);
    bool smooth() const { return m_smooth; }

    void setSkin(Skin skin);
    Skin skin() const { return m_skin; }
    QSize scaledSize(int screenScale) const;

    QSize sizeHint() const override;

signals:
    // Button values match EmulatorCore::Button / the C FFI button enum.
    void buttonChanged(int button, bool pressed);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void leaveEvent(QEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;

private:
    QPointF skinPoint(const QPointF &widgetPoint) const;
    int buttonAt(const QPointF &point) const;
    void releasePointerButton();

    QImage m_frame;
    bool m_smooth = false;
    Skin m_skin = Skin::Arduboy;
    int m_pressedButton = -1;
};
