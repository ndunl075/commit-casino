import type {CasinoSnapshot,CommittedRound} from '@commit-casino/casino-contracts';
export const STORAGE_VERSION=1;
export interface LedgerEntry {id:string;epoch:number;roundId?:string;kind:'initial'|'stake'|'return'|'refill'|'reset';delta:number;at:string}
export interface CommandRecord {id:string;fingerprint:string;result:CommittedRound|CasinoSnapshot}
export interface StoredState {version:1;snapshot:CasinoSnapshot;ledger:LedgerEntry[];commands:Record<string,CommandRecord>}
