// AudioOutput — stereo float PCM sink built on QAudioSink (push mode).
//
// The emulator produces a variable number of stereo sample pairs per frame;
// we push them straight into the sink's QIODevice. A small latency buffer in
// the sink smooths over the jitter between 60fps frame ticks.
#pragma once

#include <QAudioFormat>
#include <QObject>
#include <QScopedPointer>

class QAudioSink;
class QIODevice;

class AudioOutput : public QObject {
    Q_OBJECT
public:
    explicit AudioOutput(QObject *parent = nullptr);
    ~AudioOutput() override;

    bool start(unsigned int sampleRate = 44100);
    void stop();

    // Push `pairs` interleaved L,R float samples. No-op if muted or not started.
    void writeSamples(const float *data, int pairs);

    void setMuted(bool muted) { m_muted = muted; }
    bool muted() const { return m_muted; }

    unsigned int sampleRate() const { return m_sampleRate; }

private:
    QScopedPointer<QAudioSink> m_sink;
    QIODevice *m_io = nullptr; // owned by m_sink
    QAudioFormat m_format;
    unsigned int m_sampleRate = 44100;
    bool m_muted = false;
};
