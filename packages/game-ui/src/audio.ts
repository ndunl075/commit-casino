/** Original synthesized sounds. No samples, downloads or external assets. */
export class CasinoAudio {
  private context?: AudioContext;
  private mute = false;
  private volume = 0.4;
  configure(mute: boolean, volume: number) { this.mute = mute; this.volume = Math.max(0, Math.min(1, volume)); }
  unlock() { try { this.context ??= new AudioContext(); void this.context.resume().catch(() => undefined); } catch { /* Audio is optional. */ } }
  play(kind: 'chip' | 'spin' | 'win' | 'result') {
    if (this.mute || document.hidden || !this.context || this.context.state !== 'running') return;
    const context = this.context;
    const notes = kind === 'win' ? [523.25, 659.25, 783.99] : kind === 'chip' ? [1300, 950] : kind === 'spin' ? [220, 330, 440] : [392, 293.66];
    notes.forEach((hz, i) => {
      const t = context.currentTime + i * (kind === 'chip' ? 0.016 : 0.075);
      const oscillator = context.createOscillator(); const gain = context.createGain();
      oscillator.type = kind === 'chip' ? 'triangle' : 'sine'; oscillator.frequency.setValueAtTime(hz, t);
      gain.gain.setValueAtTime(0, t); gain.gain.linearRampToValueAtTime(this.volume * 0.1, t + 0.006); gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.18);
      oscillator.connect(gain); gain.connect(context.destination); oscillator.start(t); oscillator.stop(t + 0.19);
      oscillator.onended = () => { oscillator.disconnect(); gain.disconnect(); };
    });
  }
  destroy() { void this.context?.close().catch(() => undefined); }
}
