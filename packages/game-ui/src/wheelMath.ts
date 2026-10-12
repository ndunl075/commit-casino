export const TAU = Math.PI * 2;
export const POCKET_ANGLE = TAU / 37;
export function landingAngle(pocketIndex: number, wheelRotation: number): number { return wheelRotation + pocketIndex * POCKET_ANGLE; }
export function wheelPose(progress: number, pocketIndex: number, seed: number) {
  const t = Math.max(0, Math.min(1, progress));
  const finalWheel = TAU * 3 + (seed % 1000) / 1000 * TAU;
  const wheel = finalWheel * (1 - (1 - t) ** 3);
  const finalBall = landingAngle(pocketIndex, finalWheel);
  const ball = finalBall + TAU * 9 * (1 - t) ** 2.4;
  const drop = Math.min(1, Math.max(0, (t - 0.7) / 0.24));
  const bounce = t > 0.78 && t < 1 ? Math.sin((t - 0.78) * 95) * (1 - t) * 18 : 0;
  return { wheel, ball, radius: 196 - 39 * (1 - (1 - drop) ** 2) + bounce };
}
