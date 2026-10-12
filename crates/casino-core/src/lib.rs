//! Native financial authority. UI, animations and provider events cannot award chips.
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path, time::Duration};
use uuid::Uuid;
pub mod agent;

pub const SAFE: i64 = 9_007_199_254_740_991;
const CATALOG: &str = include_str!("../../../rules/european-v1.json");
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
impl Error {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::new(
            "STORAGE_UNAVAILABLE",
            "Local storage is unavailable. Your saved wallet has not been replaced.",
        )
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new(
            "STORAGE_UNAVAILABLE",
            "Stored data failed validation. Preserve the database for recovery.",
        )
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BetInput {
    pub bet_id: String,
    pub stake: i64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitRequest {
    pub request_id: String,
    pub expected_wallet_revision: i64,
    pub rules_version: String,
    pub bets: Vec<BetInput>,
    pub animation_mode: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    pub round_id: String,
    pub outcome: u32,
    pub pocket_index: usize,
    pub animation_seed: u32,
    pub duration_ms: u64,
    pub committed_at: String,
    pub presentation_state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    pub round_id: String,
    pub request_id: String,
    pub rules_version: String,
    pub bets: Vec<BetInput>,
    pub outcome: u32,
    pub stake: i64,
    pub gross_return: i64,
    pub net_change: i64,
    pub status: String,
    pub presentation: Presentation,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    version: String,
    wheel_order: Vec<u32>,
    red_numbers: Vec<u32>,
    bets: Vec<CatalogBet>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogBet {
    id: String,
    kind: String,
    numbers: Vec<u32>,
    profit_odds: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub mute: bool,
    pub volume: f64,
    pub notification_sound: bool,
    pub animation_mode: String,
    pub window_mode: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mute: false,
            volume: 0.6,
            notification_sound: true,
            animation_mode: "normal".into(),
            window_mode: "standard".into(),
        }
    }
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub spins: i64,
    pub total_stake: i64,
    pub gross_returns: i64,
    pub net_change: i64,
    pub largest_win: i64,
    pub longest_win_streak: i64,
    pub current_win_streak: i64,
    #[serde(default)]
    pub bet_frequencies: std::collections::BTreeMap<String, i64>,
}
pub struct Casino {
    db: Connection,
    catalog: Catalog,
}
pub struct PassiveObservation<'a> {
    pub installation: &'a str,
    pub session: &'a str,
    pub event_id: &'a str,
    pub sequence: u64,
    pub event_type: &'a str,
    pub turn_id: Option<&'a str>,
    pub observed_at_ms: u64,
    pub agent: Option<agent::AgentObservation>,
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn uuid(id: &str) -> Result<()> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| Error::new("INVALID_BET", "Request IDs must be UUIDs."))
}
fn encode<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
fn checked(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b)
        .filter(|v| (-SAFE..=SAFE).contains(v))
        .ok_or_else(|| Error::new("OVERFLOW", "Chip total exceeds the safe integer limit."))
}
/// Reject the incomplete tail of the u32 range, rather than introducing modulo bias.
pub fn uniform_pocket(mut draw: impl FnMut() -> Result<u32>) -> Result<u32> {
    let limit = (1u64 << 32) / 37 * 37;
    loop {
        let n = draw()?;
        if (n as u64) < limit {
            return Ok(n % 37);
        }
    }
}
fn secure_u32() -> Result<u32> {
    let mut bytes = [0u8; 4];
    getrandom::getrandom(&mut bytes).map_err(|_| {
        Error::new(
            "RNG_UNAVAILABLE",
            "Secure system randomness is unavailable. Try again.",
        )
    })?;
    Ok(u32::from_le_bytes(bytes))
}
impl Casino {
    pub fn integration_connected(&mut self, connected: bool) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: bool = tx.query_row(
            "SELECT connected FROM integration_state WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if old != connected {
            tx.execute(
                "UPDATE integration_state SET connected=?1 WHERE id=1",
                [connected],
            )?;
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn consumed_sequence(&self, installation: &str, session: &str) -> Result<u64> {
        Ok(self
            .db
            .query_row(
                "SELECT sequence FROM passive_sessions WHERE installation_id=?1 AND session_id=?2",
                params![installation, session],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    /// The host authenticates the envelope before calling this method. Persist only lifecycle metadata.
    pub fn ingest_passive(&mut self, observation: PassiveObservation<'_>) -> Result<bool> {
        let PassiveObservation {
            installation,
            session,
            event_id,
            sequence,
            event_type,
            turn_id,
            observed_at_ms,
            ..
        } = observation;
        if sequence > SAFE as u64 || observed_at_ms > SAFE as u64 {
            return Err(Error::new(
                "INTEGRATION_UNAVAILABLE",
                "Invalid observation bounds.",
            ));
        }
        let status = match event_type {
            "session-start" => "idle",
            "turn-start" | "tool-start" | "tool-end" => "working",
            "attention-requested" => "attention",
            "stop-candidate" => "stopped",
            "session-end" => "ended",
            "verified-run-started" | "verified-run-completed" => "working",
            _ => {
                return Err(Error::new(
                    "INTEGRATION_UNAVAILABLE",
                    "Unknown observation type.",
                ))
            }
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let consumed: u64 = tx
            .query_row(
                "SELECT sequence FROM passive_sessions WHERE installation_id=?1 AND session_id=?2",
                params![installation, session],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if sequence <= consumed {
            return Ok(false);
        }
        if sequence != consumed + 1 {
            return Err(Error::new(
                "INTEGRATION_UNAVAILABLE",
                "Observation sequence gap. Waiting for missing lifecycle data.",
            ));
        }
        tx.execute("INSERT INTO passive_events(event_id,installation_id,session_id,sequence,type,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![event_id,installation,session,sequence,event_type,observed_at_ms])?;
        tx.execute("INSERT INTO passive_sessions(installation_id,session_id,sequence,status,turn_id,last_event_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(installation_id,session_id) DO UPDATE SET sequence=excluded.sequence,status=excluded.status,turn_id=excluded.turn_id,last_event_at_ms=excluded.last_event_at_ms",params![installation,session,sequence,status,turn_id,observed_at_ms])?;
        tx.execute(
            "DELETE FROM passive_events WHERE observed_at_ms<?1",
            [observed_at_ms.saturating_sub(30 * 86_400_000)],
        )?;
        agent::observe(&tx, &observation)?;
        bump(&tx)?;
        tx.commit()?;
        Ok(true)
    }
    pub fn passive_sessions(&self) -> Result<Vec<Value>> {
        passive_sessions_db(&self.db)
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Connection::open(path.as_ref())?;
        db.busy_timeout(Duration::from_secs(3))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 3 {
            return Err(Error::new(
                "VERSION_MISMATCH",
                "This database needs a newer Commit Casino. Preserve your data.",
            ));
        }
        if version == 0 {
            if db.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get::<_, i64>(0),
            )? > 0
            {
                db.backup(
                    rusqlite::DatabaseName::Main,
                    path.as_ref().with_extension("pre-v1.sqlite3"),
                    None,
                )?;
            }
            db.execute_batch(include_str!("schema.sql"))?;
            db.execute(
                "INSERT OR IGNORE INTO settings(id,json) VALUES(1,?1)",
                [encode(&Settings::default())?],
            )?;
        }
        // Consistent SQLite backup is required before upgrading an existing wallet.
        if version == 1 && path.as_ref() != Path::new(":memory:") {
            db.backup(
                rusqlite::DatabaseName::Main,
                path.as_ref().with_extension("pre-v2.sqlite3"),
                None,
            )?;
        }
        if version < 2 {
            db.execute_batch(include_str!("integration.sql"))?;
        }
        if version == 2 && path.as_ref() != Path::new(":memory:") {
            db.backup(
                rusqlite::DatabaseName::Main,
                path.as_ref().with_extension("pre-v3.sqlite3"),
                None,
            )?;
        }
        if version < 3 {
            db.execute_batch(include_str!("agent.sql"))?;
        }
        let catalog: Catalog = serde_json::from_str(CATALOG)?;
        validate_catalog(&catalog)?;
        let mut service = Self { db, catalog };
        // Startup recovery uses the recorded result, never the wall clock or a fresh draw.
        let ids: Vec<String> = {
            let mut q = service
                .db
                .prepare("SELECT id FROM rounds WHERE status='committed'")?;
            let rows = q.query_map([], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for id in ids {
            service.settle(&id)?;
        }
        Ok(service)
    }
    pub fn snapshot(&self) -> Result<Value> {
        snapshot_db(&self.db)
    }
    pub fn commit(&mut self, request: CommitRequest) -> Result<Round> {
        self.commit_with(request, secure_u32)
    }
    fn commit_with(
        &mut self,
        request: CommitRequest,
        mut draw: impl FnMut() -> Result<u32>,
    ) -> Result<Round> {
        uuid(&request.request_id)?;
        let payload = encode(&request)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = replay(&tx, &request.request_id, "commit_classic", &payload)? {
            return Ok(serde_json::from_str(&result)?);
        }
        if request.rules_version != self.catalog.version {
            return Err(Error::new(
                "VERSION_MISMATCH",
                "Unsupported roulette rules version.",
            ));
        }
        let duration = match request.animation_mode.as_str() {
            "normal" => 6500,
            "fast" => 2000,
            "reduced-motion" => 300,
            _ => return Err(Error::new("INVALID_BET", "Unknown animation mode.")),
        };
        if tx.query_row(
            "SELECT COUNT(*) FROM rounds WHERE status='committed' OR presentation='pending'",
            [],
            |r| r.get::<_, i64>(0),
        )? > 0
        {
            return Err(Error::new(
                "ROUND_ACTIVE",
                "Finish the current round first.",
            ));
        }
        let (balance, revision, epoch): (i64, i64, i64) = tx.query_row(
            "SELECT balance,revision,epoch FROM wallet WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if request.expected_wallet_revision != revision {
            return Err(Error::new(
                "STALE_REVISION",
                "Wallet changed. Refresh before placing the bet.",
            ));
        }
        if request.bets.is_empty() || request.bets.len() > 157 {
            return Err(Error::new("INVALID_BET", "Choose at least one valid bet."));
        }
        let mut seen = HashSet::new();
        let mut stake = 0;
        let mut returns = [0i64; 37];
        for bet in &request.bets {
            if bet.stake <= 0
                || bet.stake % 10 != 0
                || bet.stake > SAFE
                || !seen.insert(&bet.bet_id)
            {
                return Err(Error::new(
                    "INVALID_BET",
                    "Stakes must be unique positive multiples of ten.",
                ));
            }
            let entry = self
                .catalog
                .bets
                .iter()
                .find(|b| b.id == bet.bet_id)
                .ok_or_else(|| Error::new("INVALID_BET", "Unknown canonical bet ID."))?;
            stake = checked(stake, bet.stake)?;
            let gross = bet
                .stake
                .checked_mul(entry.profit_odds + 1)
                .filter(|v| *v <= SAFE)
                .ok_or_else(|| {
                    Error::new("OVERFLOW", "Potential return exceeds safe integer limit.")
                })?;
            for n in &entry.numbers {
                returns[*n as usize] = checked(returns[*n as usize], gross)?;
            }
        }
        if stake > balance {
            return Err(Error::new(
                "INSUFFICIENT_FUNDS",
                "Draft exceeds the wallet balance.",
            ));
        }
        for gross in returns {
            checked(checked(balance - stake, gross)?, agent::obligations(&tx)?)?;
        }
        // Lifetime aggregates are also JS-safe: reject before debit, never during an earned credit.
        let stats_json: String =
            tx.query_row("SELECT stats FROM app_state WHERE id=1", [], |r| r.get(0))?;
        let stats: Stats = serde_json::from_str(&stats_json)?;
        let maximum = *returns.iter().max().unwrap_or(&0);
        checked(stats.spins, 1)?;
        checked(stats.total_stake, stake)?;
        checked(stats.gross_returns, maximum)?;
        checked(stats.net_change, -stake)?;
        checked(stats.net_change, maximum - stake)?;
        for bet in &request.bets {
            checked(*stats.bet_frequencies.get(&bet.bet_id).unwrap_or(&0), 1)?;
        }
        let outcome = uniform_pocket(&mut draw)?;
        let seed = draw()?;
        let round_id = Uuid::new_v4().to_string();
        let time = now();
        let round = Round {
            round_id: round_id.clone(),
            request_id: request.request_id.clone(),
            rules_version: request.rules_version,
            bets: request.bets,
            outcome,
            stake,
            gross_return: returns[outcome as usize],
            net_change: returns[outcome as usize] - stake,
            status: "committed".into(),
            presentation: Presentation {
                round_id: round_id.clone(),
                outcome,
                pocket_index: self
                    .catalog
                    .wheel_order
                    .iter()
                    .position(|n| *n == outcome)
                    .unwrap(),
                animation_seed: seed,
                duration_ms: duration,
                committed_at: time.clone(),
                presentation_state: "pending".into(),
            },
        };
        let encoded = encode(&round)?;
        tx.execute("INSERT INTO rounds(id,request_id,status,presentation,json) VALUES(?1,?2,'committed','pending',?3)",params![round_id,request.request_id,encoded])?;
        for bet in &round.bets {
            let entry = self
                .catalog
                .bets
                .iter()
                .find(|b| b.id == bet.bet_id)
                .unwrap();
            tx.execute("INSERT INTO round_bets(round_id,bet_id,stake,coverage,profit_odds) VALUES(?1,?2,?3,?4,?5)",params![round_id,bet.bet_id,bet.stake,encode(&entry.numbers)?,entry.profit_odds])?;
        }
        tx.execute("INSERT INTO wallet_ledger(id,epoch,round_id,kind,delta,created_at) VALUES(?1,?2,?3,'stake',?4,?5)",params![format!("stake:{round_id}"),epoch,round_id,-stake,time])?;
        tx.execute(
            "UPDATE wallet SET balance=balance-?1,revision=revision+1 WHERE id=1",
            [stake],
        )?;
        record(&tx, &round.request_id, "commit_classic", &payload, &encoded)?;
        bump(&tx)?;
        tx.commit()?;
        Ok(round)
    }
    pub fn settle(&mut self, id: &str) -> Result<Round> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let encoded: String =
            tx.query_row("SELECT json FROM rounds WHERE id=?1", [id], |r| r.get(0))?;
        let mut round: Round = serde_json::from_str(&encoded)?;
        if round.status == "settled" {
            return Ok(round);
        }
        let (balance, epoch): (i64, i64) =
            tx.query_row("SELECT balance,epoch FROM wallet WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        checked(balance, round.gross_return)?;
        tx.execute("INSERT INTO wallet_ledger(id,epoch,round_id,kind,delta,created_at) VALUES(?1,?2,?3,'return',?4,?5)",params![format!("return:{id}"),epoch,id,round.gross_return,now()])?;
        tx.execute(
            "UPDATE wallet SET balance=balance+?1,revision=revision+1 WHERE id=1",
            [round.gross_return],
        )?;
        round.status = "settled".into();
        tx.execute(
            "UPDATE rounds SET status='settled',json=?2 WHERE id=?1",
            params![id, encode(&round)?],
        )?;
        let stats: String =
            tx.query_row("SELECT stats FROM app_state WHERE id=1", [], |r| r.get(0))?;
        let mut stats: Stats = serde_json::from_str(&stats)?;
        stats.spins = checked(stats.spins, 1)?;
        stats.total_stake = checked(stats.total_stake, round.stake)?;
        stats.gross_returns = checked(stats.gross_returns, round.gross_return)?;
        stats.net_change = checked(stats.net_change, round.net_change)?;
        stats.largest_win = stats.largest_win.max(round.net_change);
        stats.current_win_streak = if round.net_change > 0 {
            stats.current_win_streak + 1
        } else {
            0
        };
        stats.longest_win_streak = stats.longest_win_streak.max(stats.current_win_streak);
        for bet in &round.bets {
            let count = stats.bet_frequencies.entry(bet.bet_id.clone()).or_default();
            *count = checked(*count, 1)?;
        }
        tx.execute(
            "UPDATE app_state SET stats=?1 WHERE id=1",
            [encode(&stats)?],
        )?;
        let mut unlock = vec!["first-spin"];
        if stats.spins >= 100 {
            unlock.push("hundred-spins")
        }
        for bet in &round.bets {
            if bet.bet_id == format!("straight:{}", round.outcome) {
                unlock.push("straight-winner");
                if round.outcome == 7 {
                    unlock.push("lucky-seven")
                }
            }
        }
        for key in unlock {
            tx.execute(
                "INSERT OR IGNORE INTO achievements(id,round_id,created_at) VALUES(?1,?2,?3)",
                params![key, id, now()],
            )?;
        }
        // Keep financial headers and replay identities. Only visibility has bounded retention.
        tx.execute("UPDATE rounds SET visible=0 WHERE status='settled' AND presentation='shown' AND rowid NOT IN (SELECT rowid FROM rounds ORDER BY rowid DESC LIMIT 10000)",[])?;
        bump(&tx)?;
        tx.commit()?;
        Ok(round)
    }
    pub fn acknowledge(&mut self, id: &str) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let encoded: String =
            tx.query_row("SELECT json FROM rounds WHERE id=?1", [id], |r| r.get(0))?;
        let mut round: Round = serde_json::from_str(&encoded)?;
        if round.status != "settled" {
            return Err(Error::new("ROUND_ACTIVE", "Round is still settling."));
        }
        if round.presentation.presentation_state == "shown" {
            return Ok(());
        }
        round.presentation.presentation_state = "shown".into();
        tx.execute(
            "UPDATE rounds SET presentation='shown',json=?2 WHERE id=?1",
            params![id, encode(&round)?],
        )?;
        bump(&tx)?;
        tx.commit()?;
        Ok(())
    }
    pub fn wallet_action(&mut self, id: &str, reset: bool) -> Result<Value> {
        uuid(id)?;
        let kind = if reset {
            "reset_wallet"
        } else {
            "refill_wallet"
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(previous) = replay(&tx, id, kind, "{}")? {
            return Ok(serde_json::from_str(&previous)?);
        }
        if tx.query_row(
            "SELECT (SELECT COUNT(*) FROM rounds WHERE status='committed') + (SELECT COUNT(*) FROM agent_wagers WHERE status IN ('armed','bound'))",
            [],
            |r| r.get::<_, i64>(0),
        )? > 0
        {
            return Err(Error::new(
                "ROUND_ACTIVE",
                "Committed stakes must settle before changing the wallet.",
            ));
        }
        let (balance, epoch): (i64, i64) =
            tx.query_row("SELECT balance,epoch FROM wallet WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if !reset && balance != 0 {
            return Err(Error::new(
                "INVALID_BET",
                "Free refill is available when your balance reaches zero.",
            ));
        }
        let next_epoch = if reset { checked(epoch, 1)? } else { epoch };
        tx.execute(
            "INSERT INTO wallet_ledger(id,epoch,kind,delta,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![id, next_epoch, kind, 10000 - balance, now()],
        )?;
        tx.execute(
            "UPDATE wallet SET balance=10000,revision=revision+1,epoch=?1 WHERE id=1",
            [next_epoch],
        )?;
        bump(&tx)?;
        // Freeze command result in the same transaction using a snapshot of this connection.
        let result = snapshot_db(&tx)?;
        record(&tx, id, kind, "{}", &encode(&result)?)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn update_settings(&mut self, patch: Value) -> Result<Value> {
        let current: String =
            self.db
                .query_row("SELECT json FROM settings WHERE id=1", [], |r| r.get(0))?;
        let mut settings: Value = serde_json::from_str(&current)?;
        let object = patch
            .as_object()
            .ok_or_else(|| Error::new("INVALID_BET", "Settings must be an object."))?;
        for (key, value) in object {
            if settings.get(key).is_none() {
                return Err(Error::new("INVALID_BET", "Unknown setting."));
            }
            settings[key] = value.clone()
        }
        let parsed: Settings = serde_json::from_value(settings)
            .map_err(|_| Error::new("INVALID_BET", "Invalid setting value."))?;
        if !parsed.volume.is_finite()
            || !(0.0..=1.0).contains(&parsed.volume)
            || !["normal", "fast", "reduced-motion"].contains(&parsed.animation_mode.as_str())
            || !["compact", "standard", "fullscreen"].contains(&parsed.window_mode.as_str())
        {
            return Err(Error::new("INVALID_BET", "Invalid setting value."));
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE settings SET json=?1 WHERE id=1", [encode(&parsed)?])?;
        bump(&tx)?;
        tx.commit()?;
        self.snapshot()
    }
    pub fn clear_history(&mut self, id: &str) -> Result<Value> {
        uuid(id)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(previous) = replay(&tx, id, "clear_history", "{}")? {
            return Ok(serde_json::from_str(&previous)?);
        }
        if tx.query_row(
            "SELECT (SELECT COUNT(*) FROM rounds WHERE status='committed' OR presentation='pending') + (SELECT COUNT(*) FROM agent_wagers WHERE status IN ('armed','bound'))",
            [],
            |r| r.get::<_, i64>(0),
        )? > 0
        {
            return Err(Error::new(
                "ROUND_ACTIVE",
                "Finish the active round before clearing history.",
            ));
        }
        tx.execute("UPDATE rounds SET visible=0", [])?;
        tx.execute("UPDATE agent_wagers SET visible=0", [])?;
        tx.execute(
            "UPDATE app_state SET stats=?1 WHERE id=1",
            [encode(&Stats::default())?],
        )?;
        bump(&tx)?;
        let snapshot = snapshot_db(&tx)?;
        record(&tx, id, "clear_history", "{}", &encode(&snapshot)?)?;
        tx.commit()?;
        Ok(snapshot)
    }
    pub fn reconcile(&self) -> Result<bool> {
        let balance: i64 = self
            .db
            .query_row("SELECT balance FROM wallet WHERE id=1", [], |r| r.get(0))?;
        let ledger: i64 = self.db.query_row(
            "SELECT COALESCE(SUM(delta),0) FROM wallet_ledger",
            [],
            |r| r.get(0),
        )?;
        Ok(balance == ledger)
    }
}
fn bump(tx: &Transaction) -> Result<()> {
    tx.execute("UPDATE app_state SET revision=revision+1 WHERE id=1", [])?;
    Ok(())
}
fn replay(tx: &Transaction, id: &str, kind: &str, payload: &str) -> Result<Option<String>> {
    let previous: Option<(String, String, String)> = tx
        .query_row(
            "SELECT kind,payload,result FROM commands WHERE request_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match previous {
        Some((k, p, r)) if k == kind && p == payload => Ok(Some(r)),
        Some(_) => Err(Error::new(
            "INVALID_BET",
            "Request ID was already used for different content.",
        )),
        None => Ok(None),
    }
}
fn record(tx: &Transaction, id: &str, kind: &str, payload: &str, result: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO commands(request_id,kind,payload,result) VALUES(?1,?2,?3,?4)",
        params![id, kind, payload, result],
    )?;
    Ok(())
}
fn validate_catalog(c: &Catalog) -> Result<()> {
    let expected = [
        0, 32, 15, 19, 4, 21, 2, 25, 17, 34, 6, 27, 13, 36, 11, 30, 8, 23, 10, 5, 24, 16, 33, 1,
        20, 14, 31, 9, 22, 18, 29, 7, 28, 12, 35, 3, 26,
    ];
    if c.version != "european-v1"
        || c.bets.len() != 157
        || c.wheel_order != expected
        || c.red_numbers.len() != 18
    {
        return Err(Error::new(
            "VERSION_MISMATCH",
            "Embedded rules catalog failed validation.",
        ));
    }
    let mut ids = HashSet::new();
    for b in &c.bets {
        if !ids.insert(&b.id)
            || b.numbers.is_empty()
            || b.numbers.iter().any(|n| *n > 36)
            || b.numbers.windows(2).any(|p| p[0] >= p[1])
            || b.profit_odds <= 0
            || b.kind.is_empty()
        {
            return Err(Error::new(
                "VERSION_MISMATCH",
                "Embedded bet catalog failed validation.",
            ));
        }
    }
    Ok(())
}

fn snapshot_db(db: &Connection) -> Result<Value> {
    let (balance, revision, epoch): (i64, i64, i64) = db.query_row(
        "SELECT balance,revision,epoch FROM wallet WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let state_revision: i64 =
        db.query_row("SELECT revision FROM app_state WHERE id=1", [], |r| {
            r.get(0)
        })?;
    let mut query =
        db.prepare("SELECT json FROM rounds WHERE visible=1 ORDER BY rowid DESC LIMIT 10000")?;
    let records = query
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rounds: Vec<Round> = records
        .iter()
        .map(|s| serde_json::from_str(s))
        .collect::<std::result::Result<_, _>>()?;
    let active = rounds
        .iter()
        .find(|r| r.presentation.presentation_state == "pending");
    let settings: String =
        db.query_row("SELECT json FROM settings WHERE id=1", [], |r| r.get(0))?;
    let stats: String = db.query_row("SELECT stats FROM app_state WHERE id=1", [], |r| r.get(0))?;
    let mut q = db.prepare(
        "SELECT id FROM achievements UNION SELECT id FROM agent_achievements ORDER BY id",
    )?;
    let achievements = q
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let connected: bool = db.query_row(
        "SELECT connected FROM integration_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    let agent_ready: bool = db.query_row(
        "SELECT agent_ready FROM integration_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    let outstanding:i64=db.query_row("SELECT (SELECT COALESCE(SUM(stake),0) FROM agent_wagers WHERE status IN ('armed','bound'))+(SELECT COALESCE(SUM(json_extract(json,'$.stake')),0) FROM rounds WHERE status='committed')",[],|r|r.get(0))?;
    Ok(
        json!({"revision":state_revision,"wallet":{"balance":balance,"revision":revision,"epoch":epoch},"outstandingStakes":outstanding,"activeRound":active,"history":rounds,"settings":serde_json::from_str::<Settings>(&settings)?,"stats":serde_json::from_str::<Stats>(&stats)?,"agentStats":agent::statistics(db)?,"achievements":achievements,"features":{"agentIntegration":connected,"agentRoulette":connected&&agent_ready},"sessions":passive_sessions_db(db)?}),
    )
}
fn passive_sessions_db(db: &Connection) -> Result<Vec<Value>> {
    let mut q=db.prepare("SELECT session_id,sequence,status,turn_id,last_event_at_ms FROM passive_sessions ORDER BY last_event_at_ms DESC LIMIT 100")?;
    let clock = Utc::now().timestamp_millis();
    let rows=q.query_map([],|r|{let last:i64=r.get(4)?;let status:String=r.get(2)?;let stale=clock.saturating_sub(last)>300_000&&status!="ended";Ok(json!({"sessionId":r.get::<_,String>(0)?,"sequence":r.get::<_,i64>(1)?,"status":if stale{"stale"}else{status.as_str()},"turnId":r.get::<_,Option<String>>(3)?,"lastEventAtMs":last}))})?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(rows)
}

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod tests;
