// Sounds made in code (Web Audio) - no sound files to license. Off by default (settings).

let ctx: AudioContext | null = null;

function audio(): AudioContext | null {
  try {
    ctx ??= new AudioContext();
    return ctx;
  } catch {
    return null;
  }
}

/** Quiet clatter of split-flap tiles - new offers on the board. */
export function playClack(): void {
  const a = audio();
  if (!a) return;
  const start = a.currentTime;
  for (let i = 0; i < 5; i++) {
    const length = Math.floor(a.sampleRate * 0.018);
    const buffer = a.createBuffer(1, length, a.sampleRate);
    const data = buffer.getChannelData(0);
    for (let j = 0; j < length; j++) data[j] = (Math.random() * 2 - 1) * (1 - j / length) ** 3;
    const source = a.createBufferSource();
    source.buffer = buffer;
    const filter = a.createBiquadFilter();
    filter.type = "bandpass";
    filter.frequency.value = 1800 + i * 150;
    const gain = a.createGain();
    gain.gain.value = 0.18;
    source.connect(filter).connect(gain).connect(a.destination);
    source.start(start + i * 0.055);
  }
}

/** Short, soft gong - an interview in an hour. */
export function playGong(): void {
  const a = audio();
  if (!a) return;
  const start = a.currentTime;
  for (const [freq, level] of [
    [196, 0.22],
    [392, 0.08],
    [588, 0.04],
  ] as const) {
    const osc = a.createOscillator();
    osc.type = "sine";
    osc.frequency.value = freq;
    const gain = a.createGain();
    gain.gain.setValueAtTime(level, start);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + 1.8);
    osc.connect(gain).connect(a.destination);
    osc.start(start);
    osc.stop(start + 1.9);
  }
}
