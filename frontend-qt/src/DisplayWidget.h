// DisplayWidget — paints the 128x64 emulator framebuffer, scaled to fit while
// preserving the 2:1 aspect ratio, with letterboxing. Nearest-neighbour scaling
// keeps the pixel-art crisp.
#pragma once

#include <QImage>
#include <QWidget>

class DisplayWidget : public QWidget {
    Q_OBJECT
public:
    explicit DisplayWidget(QWidget *parent = nullptr);

    // Update the displayed image (a shallow copy; the pixels are memcpy'd by the
    // caller each frame). Triggers a repaint.
    void setFrame(const QImage &img);

    // Smooth (bilinear) vs. crisp (nearest) scaling.
    void setSmooth(bool smooth);
    bool smooth() const { return m_smooth; }

    QSize sizeHint() const override;

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    QImage m_frame;
    bool m_smooth = false;
};
