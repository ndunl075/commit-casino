import { useEffect, useRef, useState } from 'react';
import { Application, Container, Graphics, Text } from 'pixi.js';
import { catalog } from '@commit-casino/roulette-engine';
import type { CommittedRound } from '@commit-casino/casino-contracts';
import { colorOf } from './geometry';
import { POCKET_ANGLE, TAU, wheelPose } from './wheelMath';

type Scene = { app: Application; wheel: Container; ball: Container };
export function Wheel({ round, reducedMotion, onComplete }: { round: CommittedRound | null; reducedMotion: boolean; onComplete: (round: CommittedRound) => void }) {
  const host = useRef<HTMLDivElement>(null);
  const scene = useRef<Scene | null>(null);
  const latest = useRef({ round, reducedMotion, onComplete }); latest.current = { round, reducedMotion, onComplete };
  const [ready, setReady] = useState(0);
  const [fallback, setFallback] = useState(false);
  useEffect(() => {
    let disposed = false; let app: Application | undefined;
    void (async () => {
      app = new Application();
      await app.init({ width: 480, height: 480, antialias: true, backgroundAlpha: 0, resolution: Math.min(window.devicePixelRatio || 1, 2), autoDensity: true, autoStart: false, preference: 'webgl', powerPreference: 'low-power' });
      if (disposed) { app.destroy(true, { children: true }); return; }
      app.canvas.setAttribute('aria-hidden', 'true'); host.current?.appendChild(app.canvas);
      const stage = new Container(); stage.position.set(240, 240); app.stage.addChild(stage);
      const body = new Graphics();
      body.circle(0, 4, 232).fill({ color: 0x000000, alpha: 0.45 });
      body.circle(0, 0, 228).fill(0x584531).circle(0, 0, 224).fill(0x32271e);
      body.circle(0, 0, 220).stroke({ color: 0xc3a366, width: 3 });
      body.circle(0, 0, 213).stroke({ color: 0x775c38, width: 7 });
      body.circle(0, 0, 207).fill(0x191e19).circle(0, 0, 201).stroke({ color: 0x8e774b, width: 1 });
      stage.addChild(body);
      const wheel = new Container(); stage.addChild(wheel);
      for (let i = 0; i < 37; i++) {
        const n = catalog.wheelOrder[i]!; const a = i * POCKET_ANGLE; const start = a - POCKET_ANGLE / 2; const end = a + POCKET_ANGLE / 2;
        const pocket = new Graphics().moveTo(Math.cos(start) * 128, Math.sin(start) * 128).lineTo(Math.cos(start) * 187, Math.sin(start) * 187).arc(0, 0, 187, start, end).lineTo(Math.cos(end) * 128, Math.sin(end) * 128).arc(0, 0, 128, end, start, true).closePath().fill(n === 0 ? 0x20784a : colorOf(n) === 'red' ? 0xad3639 : 0x141c19).stroke({ color: 0xbea56e, width: 0.7 });
        wheel.addChild(pocket);
        const label = new Text({ text: String(n), style: { fontFamily: 'Georgia, serif', fontSize: 13, fill: 0xf5efdf, fontWeight: 'bold' } });
        label.anchor.set(0.5); label.position.set(Math.cos(a) * 175, Math.sin(a) * 175); label.rotation = a + Math.PI / 2; wheel.addChild(label);
        const divider = new Graphics().moveTo(Math.cos(start) * 131, Math.sin(start) * 131).lineTo(Math.cos(start) * 162, Math.sin(start) * 162).stroke({ color: 0xe3c991, width: 1.8 }); wheel.addChild(divider);
      }
      const hub = new Graphics().circle(0, 0, 128).fill(0x624833).circle(0, 0, 124).stroke({ color: 0xb8945b, width: 2 }).circle(0, 0, 119).fill(0x352d24).circle(0, 0, 110).stroke({ color: 0x766044, width: 1 }).circle(0, 0, 95).fill(0x403425).circle(0, 0, 94).stroke({ color: 0x5c4932, width: 2 });
      wheel.addChild(hub);
      for (let i = 0; i < 8; i++) { const a = i * TAU / 8; wheel.addChild(new Graphics().moveTo(Math.cos(a) * 35, Math.sin(a) * 35).lineTo(Math.cos(a) * 111, Math.sin(a) * 111).stroke({ color: 0x766044, width: 0.8, alpha: 0.6 })); }
      const spindle = new Graphics().roundRect(-64, -5, 128, 10, 5).fill(0xb59963).roundRect(-5, -64, 10, 128, 5).fill(0xb59963).circle(0, 0, 24).fill(0x29261e).circle(0, 0, 18).fill(0xc5ac76).circle(-3, -4, 9).fill(0xe4d3a1);
      wheel.addChild(spindle);
      const ball = new Container(); ball.addChild(new Graphics().circle(2, 3, 6).fill({ color: 0x000000, alpha: 0.6 }).circle(0, 0, 5).fill(0xece9dc).circle(-1.5, -1.5, 2).fill(0xffffff)); ball.position.set(196, 0); stage.addChild(ball);
      const lose = () => setFallback(true); app.canvas.addEventListener('webglcontextlost', lose);
      scene.current = { app, wheel, ball }; app.render(); setReady(v => v + 1);
    })().catch(() => { if (!disposed) setFallback(true); });
    return () => { disposed = true; scene.current = null; if (app?.renderer) app.destroy(true, { children: true }); };
  }, []);
  useEffect(() => {
    if (!round) return;
    const elapsed = Math.max(0, Date.now() - Date.parse(round.presentation.committedAt));
    const duration = reducedMotion || round.status === 'settled' ? Math.min(200, round.presentation.durationMs) : round.presentation.durationMs;
    const start = performance.now() - (round.status === 'settled' ? duration : elapsed);
    let frame = 0; let complete = false;
    const tick = () => {
      const t = Math.min(1, (performance.now() - start) / Math.max(duration, 1));
      const pose = wheelPose(reducedMotion ? 1 : t, round.presentation.pocketIndex, round.presentation.animationSeed);
      const s = scene.current;
      if (s && !fallback) { s.wheel.rotation = pose.wheel; s.ball.position.set(Math.cos(pose.ball) * pose.radius, Math.sin(pose.ball) * pose.radius); s.app.render(); }
      if (t >= 1) { if (!complete) { complete = true; latest.current.onComplete(round); } return; }
      if (!document.hidden) frame = requestAnimationFrame(tick);
    };
    const visibility = () => { cancelAnimationFrame(frame); if (!document.hidden) tick(); };
    tick(); document.addEventListener('visibilitychange', visibility);
    return () => { cancelAnimationFrame(frame); document.removeEventListener('visibilitychange', visibility); };
  }, [round?.roundId, ready, reducedMotion, fallback]);
  return <div className="wheel-view" ref={host} role="img" aria-label="European roulette wheel, 37 pockets">
    {fallback ? <div className="wheel-fallback"><span>EUROPEAN ROULETTE</span><strong>{round?.outcome ?? '0'}</strong><small>Simple wheel display</small></div> : null}
    <div className="wheel-pointer" aria-hidden="true" />
  </div>;
}
