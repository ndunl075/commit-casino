import {z} from 'zod';
export const chipsSchema=z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
export const betInputSchema=z.object({betId:z.string().min(1),stake:chipsSchema.positive().refine(n=>n%10===0)}).strict();
export type BetInput=z.infer<typeof betInputSchema>;
export const settingsSchema=z.object({mute:z.boolean(),volume:z.number().min(0).max(1),notificationSound:z.boolean(),animationMode:z.enum(['normal','fast','reduced-motion']),windowMode:z.enum(['compact','standard','fullscreen'])}).strict();
export type Settings=z.infer<typeof settingsSchema>;
export type SettingsPatch=Partial<Settings>;
export const commitClassicSchema=z.object({requestId:z.uuid(),expectedWalletRevision:chipsSchema,rulesVersion:z.literal('european-v1'),bets:z.array(betInputSchema).min(1).max(157),animationMode:settingsSchema.shape.animationMode}).strict();
export type CommitClassicRequest=z.infer<typeof commitClassicSchema>;
export interface RoundPresentation {roundId:string;outcome:number;pocketIndex:number;animationSeed:number;durationMs:number;committedAt:string;presentationState:'pending'|'shown'}
export interface CommittedRound {roundId:string;requestId:string;rulesVersion:'european-v1';bets:BetInput[];outcome:number;stake:number;grossReturn:number;netChange:number;status:'committed'|'settled';presentation:RoundPresentation}
export interface Statistics {spins:number;totalStake:number;grossReturns:number;netChange:number;largestWin:number;longestWinStreak:number;currentWinStreak:number}
export interface CasinoSnapshot {revision:number;wallet:{balance:number;revision:number;epoch:number};activeRound:CommittedRound|null;history:CommittedRound[];settings:Settings;stats:Statistics;achievements:string[];features:{agentIntegration:boolean;agentRoulette:boolean};sessions:unknown[]}
export interface CasinoService {getSnapshot():Promise<CasinoSnapshot>;commitClassic(request:CommitClassicRequest):Promise<CommittedRound>;acknowledgePresentation(roundId:string):Promise<void>;resetWallet(requestId:string):Promise<CasinoSnapshot>;refillWallet(requestId:string):Promise<CasinoSnapshot>;updateSettings(patch:SettingsPatch):Promise<CasinoSnapshot>;clearHistory(requestId:string):Promise<CasinoSnapshot>;subscribe(listener:(snapshot:CasinoSnapshot)=>void):()=>void}
export type ErrorCode='INSUFFICIENT_FUNDS'|'STALE_REVISION'|'ROUND_ACTIVE'|'INVALID_BET'|'OVERFLOW'|'STORAGE_UNAVAILABLE'|'RNG_UNAVAILABLE'|'INTEGRATION_UNAVAILABLE'|'VERSION_MISMATCH'|'REQUEST_CONFLICT';
export class CasinoError extends Error {constructor(public code:ErrorCode,message:string=code){super(message);this.name='CasinoError';}}
export const defaultSettings:Settings={mute:false,volume:0.5,notificationSound:true,animationMode:'normal',windowMode:'standard'};
export const emptyStats=():Statistics=>({spins:0,totalStake:0,grossReturns:0,netChange:0,largestWin:0,longestWinStreak:0,currentWinStreak:0});
