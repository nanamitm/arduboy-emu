#include "AudioOutput.h"

#include <QAudioSink>
#include <QIODevice>
#include <QMediaDevices>

AudioOutput::AudioOutput(QObject *parent) : QObject(parent) {}

AudioOutput::~AudioOutput() { stop(); }

bool AudioOutput::start(unsigned int sampleRate) {
    m_sampleRate = sampleRate;
    m_format.setSampleRate(static_cast<int>(sampleRate));
    m_format.setChannelCount(2);
    m_format.setSampleFormat(QAudioFormat::Float);

    const QAudioDevice dev = QMediaDevices::defaultAudioOutput();
    if (dev.isNull())
        return false;
    if (!dev.isFormatSupported(m_format)) {
        // Fall back to the device's preferred format's sample rate if needed.
        m_format = dev.preferredFormat();
        m_format.setChannelCount(2);
        m_format.setSampleFormat(QAudioFormat::Float);
        m_sampleRate = static_cast<unsigned int>(m_format.sampleRate());
    }

    m_sink.reset(new QAudioSink(dev, m_format));
    // ~4 frames of latency buffer to absorb frame-timer jitter.
    m_sink->setBufferSize(static_cast<int>(m_sampleRate) * 2 * sizeof(float) / 15);
    m_io = m_sink->start();
    return m_io != nullptr;
}

void AudioOutput::stop() {
    if (m_sink) {
        m_sink->stop();
        m_sink.reset();
    }
    m_io = nullptr;
}

void AudioOutput::writeSamples(const float *data, int pairs) {
    if (!m_io || m_muted || pairs <= 0)
        return;
    const qint64 bytes = static_cast<qint64>(pairs) * 2 * sizeof(float);
    m_io->write(reinterpret_cast<const char *>(data), bytes);
}
