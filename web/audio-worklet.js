// AudioWorklet that plays interleaved stereo f32 samples pushed from the main
// thread. A simple ring buffer absorbs the jitter between animation frames.
// No SharedArrayBuffer needed — samples arrive via port messages — so the page
// works without cross-origin isolation (COOP/COEP).

class ArduboyAudio extends AudioWorkletProcessor {
  constructor() {
    super();
    // ~0.5s of stereo headroom at 48 kHz.
    this.capacity = 48000;              // frames (stereo pairs)
    this.left = new Float32Array(this.capacity);
    this.right = new Float32Array(this.capacity);
    this.readIdx = 0;
    this.writeIdx = 0;
    this.size = 0;                      // buffered pairs

    this.port.onmessage = (e) => {
      const data = e.data;             // interleaved L,R Float32Array
      const pairs = data.length >> 1;
      for (let i = 0; i < pairs; i++) {
        if (this.size >= this.capacity) break; // drop on overflow
        this.left[this.writeIdx] = data[i * 2];
        this.right[this.writeIdx] = data[i * 2 + 1];
        this.writeIdx = (this.writeIdx + 1) % this.capacity;
        this.size++;
      }
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const l = out[0];
    const r = out[1] || out[0];
    const n = l.length;
    for (let i = 0; i < n; i++) {
      if (this.size > 0) {
        l[i] = this.left[this.readIdx];
        r[i] = this.right[this.readIdx];
        this.readIdx = (this.readIdx + 1) % this.capacity;
        this.size--;
      } else {
        l[i] = 0;
        r[i] = 0;
      }
    }
    return true;
  }
}

registerProcessor('arduboy-audio', ArduboyAudio);
