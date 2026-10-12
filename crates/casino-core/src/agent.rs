//! Agent wallet mutations consume only host-authenticated evidence. No renderer settlement command exists.
use super::*;
use agent_core::{Boundary, Category, Classification, TurnEvidence, VerificationRun};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArmAgentRequest {
    pub request_id: String,
    pub expected_wallet_revision: i64,
    pub session_id: String,
    pub prediction: Category,
    pub stake: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWager {
    pub wager_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub prediction: Category,
    pub stake: i64,
    pub status: String,
    pub outcome: Option<Category>,
    pub gross_return: i64,
    pub reason: String,
    pub armed_at: String,
    pub armed_at_ms: u64,
    pub expires_at_ms: u64,
    pub installation_id: String,
    pub watermark: u64,
    pub link_id: String,
    pub required_suite_ids: Vec<String>,
    pub classifier_version: String,
    pub payout_version: String,
    pub boundary_sequence: Option<u64>,
}
pub struct ArmAuthority {
    pub installation_id: String,
    pub link_id: String,
    pub watermark: u64,
    pub required_suite_ids: Vec<String>,
    pub observed_at_ms: u64,
}
#[derive(Clone)]
pub struct AgentObservation {
    pub tool_id: Option<String>,
    pub effect: Option<String>,
    pub verification: Option<VerificationRun>,
    pub journal_gap: bool,
    pub link_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceState {
    evidence: TurnEvidence,
    active_tools: HashSet<String>,
    #[serde(default)]
    possible_mutation_tools: HashSet<String>,
    active_runs: HashSet<String>,
}

fn get(tx: &Connection, id: &str) -> Result<AgentWager> {
    let raw: String = tx.query_row("SELECT json FROM agent_wagers WHERE id=?1", [id], |r| {
        r.get(0)
    })?;
    Ok(serde_json::from_str(&raw)?)
}
fn put(tx: &Transaction, wager: &AgentWager) -> Result<()> {
    tx.execute(
        "UPDATE agent_wagers SET turn_id=?2,status=?3,expires_at_ms=?4,json=?5 WHERE id=?1",
        params![
            wager.wager_id,
            wager.turn_id,
            wager.status,
            wager.expires_at_ms,
            encode(wager)?
        ],
    )?;
    Ok(())
}
fn pending(tx: &Connection, installation: &str, session: &str) -> Result<Option<AgentWager>> {
    let raw:Option<String>=tx.query_row("SELECT json FROM agent_wagers WHERE installation_id=?1 AND session_id=?2 AND status IN ('armed','bound')",params![installation,session],|r|r.get(0)).optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(Error::from))
        .transpose()
}
pub(crate) fn obligations(db: &Connection) -> Result<i64> {
    Ok(db.query_row(
        "SELECT COALESCE(SUM(stake*2),0) FROM agent_wagers WHERE status IN ('armed','bound')",
        [],
        |r| r.get(0),
    )?)
}
fn classic_obligations(tx: &Connection) -> Result<i64> {
    let mut query = tx.prepare("SELECT json FROM rounds WHERE status='committed'")?;
    let mut total = 0;
    for row in query.query_map([], |r| r.get::<_, String>(0))? {
        let round: Round = serde_json::from_str(&row?)?;
        total = checked(total, round.gross_return)?;
    }
    Ok(total)
}
impl Casino {
    pub fn agent_arm_replay(&self, request: &ArmAgentRequest) -> Result<Option<AgentWager>> {
        uuid(&request.request_id)?;
        let previous: Option<(String, String, String)> = self
            .db
            .query_row(
                "SELECT kind,payload,result FROM commands WHERE request_id=?1",
                [&request.request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match previous {
            Some((kind, payload, result)) if kind == "arm_agent" && payload == encode(request)? => {
                Ok(Some(serde_json::from_str(&result)?))
            }
            Some(_) => Err(Error::new(
                "REQUEST_CONFLICT",
                "Request ID was already used for different content.",
            )),
            None => Ok(None),
        }
    }
    pub fn arm_agent(
        &mut self,
        request: ArmAgentRequest,
        authority: ArmAuthority,
    ) -> Result<AgentWager> {
        uuid(&request.request_id)?;
        let payload = encode(&request)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(previous) = replay(&tx, &request.request_id, "arm_agent", &payload)? {
            return Ok(serde_json::from_str(&previous)?);
        }
        if request.prediction == Category::Unknown
            || request.stake <= 0
            || request.stake % 10 != 0
            || request.stake > SAFE / 2
            || authority.required_suite_ids.is_empty()
            || authority.link_id.is_empty()
        {
            return Err(Error::new("INVALID_BET","Choose a supported prediction and a positive multiple of ten; verified suites are required."));
        }
        if pending(&tx, &authority.installation_id, &request.session_id)?.is_some() {
            return Err(Error::new(
                "ROUND_ACTIVE",
                "This session already has an unresolved prediction.",
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
                "Wallet changed. Refresh before arming.",
            ));
        }
        if request.stake > balance {
            return Err(Error::new(
                "INSUFFICIENT_FUNDS",
                "Prediction exceeds available chips.",
            ));
        }
        let existing = checked(obligations(&tx)?, classic_obligations(&tx)?)?;
        checked(
            checked(balance - request.stake, existing)?,
            request.stake * 2,
        )?;
        let aggregates = statistics(&tx)?;
        let pending_stakes: i64 = tx.query_row(
            "SELECT COALESCE(SUM(stake),0) FROM agent_wagers WHERE status IN ('armed','bound')",
            [],
            |r| r.get(0),
        )?;
        let reserved = checked(pending_stakes, request.stake)?;
        checked(aggregates["stakes"].as_i64().unwrap_or(0), reserved)?;
        checked(
            aggregates["grossReturns"].as_i64().unwrap_or(0),
            reserved
                .checked_mul(2)
                .filter(|v| *v <= SAFE)
                .ok_or_else(|| {
                    Error::new("OVERFLOW", "Agent aggregate return exceeds safe limits.")
                })?,
        )?;
        checked(aggregates["netChange"].as_i64().unwrap_or(0), reserved)?;
        checked(aggregates["netChange"].as_i64().unwrap_or(0), -reserved)?;
        let wager = AgentWager {
            wager_id: Uuid::new_v4().to_string(),
            session_id: request.session_id,
            turn_id: None,
            prediction: request.prediction,
            stake: request.stake,
            status: "armed".into(),
            outcome: None,
            gross_return: 0,
            reason: "Waiting for the next observed prompt boundary.".into(),
            armed_at: now(),
            armed_at_ms: authority.observed_at_ms,
            expires_at_ms: authority
                .observed_at_ms
                .saturating_add(agent_core::PENDING_TIMEOUT_MS),
            installation_id: authority.installation_id,
            watermark: authority.watermark,
            link_id: authority.link_id,
            required_suite_ids: authority.required_suite_ids,
            classifier_version: agent_core::CLASSIFIER_VERSION.into(),
            payout_version: agent_core::PAYOUT_VERSION.into(),
            boundary_sequence: None,
        };
        tx.execute("INSERT INTO agent_wagers(id,installation_id,session_id,status,stake,watermark,expires_at_ms,json) VALUES(?1,?2,?3,'armed',?4,?5,?6,?7)",params![wager.wager_id,wager.installation_id,wager.session_id,wager.stake,wager.watermark,wager.expires_at_ms,encode(&wager)?])?;
        tx.execute("INSERT INTO wallet_ledger(id,epoch,kind,delta,created_at) VALUES(?1,?2,'agent-stake',?3,?4)",params![format!("agent-stake:{}",wager.wager_id),epoch,-wager.stake,now()])?;
        tx.execute(
            "UPDATE wallet SET balance=balance-?1,revision=revision+1 WHERE id=1",
            [wager.stake],
        )?;
        record(
            &tx,
            &request.request_id,
            "arm_agent",
            &payload,
            &encode(&wager)?,
        )?;
        bump(&tx)?;
        tx.commit()?;
        Ok(wager)
    }
    pub fn agent_wager(&self, id: &str) -> Result<AgentWager> {
        get(&self.db, id)
    }
    pub fn agent_wagers(&self) -> Result<Vec<AgentWager>> {
        let mut q = self.db.prepare(
            "SELECT json FROM agent_wagers WHERE visible=1 ORDER BY rowid DESC LIMIT 10000",
        )?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|s| serde_json::from_str(s).map_err(Error::from))
            .collect()
    }
    pub fn agent_ready(&mut self, ready: bool) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: bool = tx.query_row(
            "SELECT agent_ready FROM integration_state WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if ready != old {
            tx.execute(
                "UPDATE integration_state SET agent_ready=?1 WHERE id=1",
                [ready],
            )?;
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn agent_has_active_runs(&self, installation: &str, session: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT COUNT(*) FROM agent_observed_runs WHERE installation_id=?1 AND session_id=?2 AND status!='complete'",params![installation,session],|r|r.get::<_,i64>(0))?>0)
    }
    pub fn void_agent(
        &mut self,
        request_id: &str,
        wager_id: &str,
        cancel_only: bool,
    ) -> Result<AgentWager> {
        uuid(request_id)?;
        let payload = encode(&(wager_id, cancel_only))?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(previous) = replay(&tx, request_id, "void_agent", &payload)? {
            return Ok(serde_json::from_str(&previous)?);
        }
        let mut wager = get(&tx, wager_id)?;
        if cancel_only && wager.status != "armed" {
            return Err(Error::new(
                "ROUND_ACTIVE",
                "A bound prediction can be explicitly voided but cannot be canceled.",
            ));
        }
        if wager.status == "armed" || wager.status == "bound" {
            settle(
                &tx,
                &mut wager,
                Category::Unknown,
                if cancel_only {
                    "Canceled before binding."
                } else {
                    "Explicitly voided for a stake refund."
                },
                None,
            )?;
        }
        record(&tx, request_id, "void_agent", &payload, &encode(&wager)?)?;
        bump(&tx)?;
        tx.commit()?;
        Ok(wager)
    }
    pub fn expire_agent(&mut self, now_ms: u64) -> Result<bool> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: Vec<String> = {
            let mut q=tx.prepare("SELECT id FROM agent_wagers WHERE status IN ('armed','bound') AND expires_at_ms<=?1")?;
            let rows = q
                .query_map([now_ms], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            rows
        };
        for id in &ids {
            let mut wager = get(&tx, id)?;
            settle(
                &tx,
                &mut wager,
                Category::Unknown,
                "No valid terminal boundary within fifteen minutes; stake refunded.",
                None,
            )?;
        }
        if !ids.is_empty() {
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(!ids.is_empty())
    }
}
fn settle(
    tx: &Transaction,
    wager: &mut AgentWager,
    outcome: Category,
    reason: &str,
    boundary: Option<u64>,
) -> Result<()> {
    if wager.status != "armed" && wager.status != "bound" {
        return Ok(());
    }
    let settlement =
        agent_core::settle_prediction(wager.prediction, outcome, wager.stake as u64)
            .map_err(|_| Error::new("OVERFLOW", "Agent payout exceeds safe chip bounds."))?;
    let gross = settlement.gross_return as i64;
    let (balance, epoch): (i64, i64) =
        tx.query_row("SELECT balance,epoch FROM wallet WHERE id=1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    checked(balance, gross)?;
    tx.execute(
        "INSERT INTO wallet_ledger(id,epoch,kind,delta,created_at) VALUES(?1,?2,?3,?4,?5)",
        params![
            format!("agent-return:{}", wager.wager_id),
            epoch,
            if settlement.void {
                "agent-refund"
            } else {
                "agent-return"
            },
            gross,
            now()
        ],
    )?;
    tx.execute(
        "UPDATE wallet SET balance=balance+?1,revision=revision+1 WHERE id=1",
        [gross],
    )?;
    wager.status = if settlement.void { "void" } else { "settled" }.into();
    wager.outcome = Some(outcome);
    wager.gross_return = gross;
    wager.reason = reason.into();
    wager.boundary_sequence = boundary;
    put(tx, wager)?;
    if !settlement.void && gross > wager.stake {
        let key = if outcome == Category::Comeback {
            Some("comeback-prediction")
        } else {
            None
        };
        if let Some(key) = key {
            tx.execute(
                "INSERT OR IGNORE INTO agent_achievements(id,wager_id,created_at) VALUES(?1,?2,?3)",
                params![key, wager.wager_id, now()],
            )?;
        }
    }
    let correct:i64=tx.query_row("SELECT COUNT(*) FROM agent_wagers WHERE status='settled' AND json_extract(json,'$.grossReturn')>stake",[],|r|r.get(0))?;
    if correct >= 25 {
        tx.execute("INSERT OR IGNORE INTO agent_achievements(id,wager_id,created_at) VALUES('agent-whisperer',?1,?2)",params![wager.wager_id,now()])?;
    }
    Ok(())
}
pub(crate) fn statistics(db: &Connection) -> Result<Value> {
    let mut q = db.prepare(
        "SELECT json FROM agent_wagers WHERE visible=1 AND status IN ('settled','void')",
    )?;
    let mut wagers = 0i64;
    let mut correct = 0i64;
    let mut incorrect = 0i64;
    let mut voids = 0i64;
    let mut stakes = 0;
    let mut gross = 0;
    for row in q.query_map([], |r| r.get::<_, String>(0))? {
        let w: AgentWager = serde_json::from_str(&row?)?;
        wagers = checked(wagers, 1)?;
        stakes = checked(stakes, w.stake)?;
        gross = checked(gross, w.gross_return)?;
        if w.status == "void" {
            voids += 1
        } else if w.gross_return > w.stake {
            correct += 1
        } else {
            incorrect += 1
        }
    }
    Ok(
        json!({"wagers":wagers,"correct":correct,"incorrect":incorrect,"voids":voids,"stakes":stakes,"grossReturns":gross,"netChange":gross-stakes}),
    )
}
pub(crate) fn observe(tx: &Transaction, passive: &PassiveObservation<'_>) -> Result<()> {
    if let Some(run) = passive.agent.as_ref().and_then(|a| a.verification.as_ref()) {
        if passive.event_type == "verified-run-started" {
            tx.execute("INSERT OR IGNORE INTO agent_observed_runs(run_id,installation_id,session_id,turn_id,status) VALUES(?1,?2,?3,?4,'started')",params![run.run_id,passive.installation,passive.session,run.turn_id])?;
        }
        if passive.event_type == "verified-run-completed" {
            tx.execute("INSERT INTO agent_observed_runs(run_id,installation_id,session_id,turn_id,status) VALUES(?1,?2,?3,?4,'incomplete') ON CONFLICT(run_id) DO UPDATE SET status=CASE WHEN installation_id=excluded.installation_id AND session_id=excluded.session_id AND turn_id=excluded.turn_id THEN 'complete' ELSE 'incomplete' END",params![run.run_id,passive.installation,passive.session,run.turn_id])?;
        }
    }
    let Some(mut wager) = pending(tx, passive.installation, passive.session)? else {
        return Ok(());
    };
    if passive.observed_at_ms >= wager.expires_at_ms {
        settle(
            tx,
            &mut wager,
            Category::Unknown,
            "Prediction expired; stake refunded.",
            None,
        )?;
        return Ok(());
    }
    let agent = passive.agent.as_ref();
    if wager.status == "armed" {
        if passive.event_type == "session-end" {
            settle(
                tx,
                &mut wager,
                Category::Unknown,
                "Session closed before the next prompt; stake refunded.",
                Some(passive.sequence),
            )?;
            return Ok(());
        }
        if passive.event_type != "turn-start" || passive.sequence <= wager.watermark {
            return Ok(());
        }
        let Some(turn) = passive.turn_id else {
            settle(
                tx,
                &mut wager,
                Category::Unknown,
                "Prompt attribution unavailable; stake refunded.",
                None,
            )?;
            return Ok(());
        };
        if agent.is_none_or(|a| a.journal_gap || a.link_id.as_deref() != Some(&wager.link_id)) {
            settle(
                tx,
                &mut wager,
                Category::Unknown,
                "Prompt attribution or sequence coverage uncertain; stake refunded.",
                None,
            )?;
            return Ok(());
        }
        wager.status = "bound".into();
        wager.turn_id = Some(turn.into());
        wager.expires_at_ms = passive
            .observed_at_ms
            .saturating_add(agent_core::PENDING_TIMEOUT_MS);
        wager.reason =
            "Bound to the observed coding turn. Waiting for a verified terminal boundary.".into();
        put(tx, &wager)?;
        let state = EvidenceState {
            evidence: TurnEvidence {
                session_id: wager.session_id.clone(),
                turn_id: turn.into(),
                required_suite_ids: wager.required_suite_ids.clone(),
                required_build_ids: vec![],
                runs: vec![],
                boundary: None,
                boundary_sequence: 0,
                consumed_sequence: passive.sequence,
                latest_stop_sequence: None,
                last_mutation_sequence: passive.sequence,
                continuous_coverage: true,
                attribution_valid: true,
                mutation_tracking_complete: true,
                unaccounted_activity: false,
                unresolved_attention: false,
            },
            active_tools: HashSet::new(),
            possible_mutation_tools: HashSet::new(),
            active_runs: HashSet::new(),
        };
        tx.execute(
            "INSERT INTO agent_evidence(wager_id,json) VALUES(?1,?2)",
            params![wager.wager_id, encode(&state)?],
        )?;
        return Ok(());
    }
    let raw: String = tx.query_row(
        "SELECT json FROM agent_evidence WHERE wager_id=?1",
        [&wager.wager_id],
        |r| r.get(0),
    )?;
    let mut state: EvidenceState = serde_json::from_str(&raw)?;
    state.evidence.consumed_sequence = passive.sequence;
    if agent.is_none_or(|a| a.journal_gap) {
        state.evidence.continuous_coverage = false;
    }
    if agent.is_none_or(|a| a.link_id.as_deref() != Some(&wager.link_id)) {
        state.evidence.attribution_valid = false;
    }
    if passive.event_type == "turn-start" || passive.event_type == "session-end" {
        if passive.event_type == "turn-start" && passive.turn_id == wager.turn_id.as_deref() {
            state.evidence.attribution_valid = false;
        }
        state.evidence.boundary = Some(if passive.event_type == "turn-start" {
            Boundary::NextTurn
        } else {
            Boundary::SessionEnd
        });
        state.evidence.boundary_sequence = passive.sequence;
        state.evidence.unaccounted_activity =
            !state.active_tools.is_empty() || !state.active_runs.is_empty();
        let Classification {
            outcome, reason, ..
        } = agent_core::classify_turn(&state.evidence);
        settle(tx, &mut wager, outcome, &reason, Some(passive.sequence))?;
    } else {
        if passive.turn_id != wager.turn_id.as_deref() {
            state.evidence.attribution_valid = false;
        }
        match passive.event_type {
            "tool-start" => {
                state.evidence.latest_stop_sequence = None;
                state.evidence.unresolved_attention = false;
                if let Some(a) = agent {
                    if let Some(tool) = &a.tool_id {
                        if !state.active_tools.insert(tool.clone()) {
                            state.evidence.continuous_coverage = false;
                        }
                    } else {
                        state.evidence.mutation_tracking_complete = false;
                    }
                    if a.effect.as_deref() != Some("read") {
                        state.evidence.last_mutation_sequence = passive.sequence;
                        if let Some(tool) = &a.tool_id {
                            state.possible_mutation_tools.insert(tool.clone());
                        }
                    }
                }
            }
            "tool-end" => {
                // Shell tools can mutate after running their contained checks. Only
                // separately observed verification after completion remains fresh.
                let started_mutation = agent
                    .and_then(|a| a.tool_id.as_ref())
                    .is_some_and(|id| state.possible_mutation_tools.remove(id));
                let possible_mutation =
                    started_mutation || agent.is_none_or(|a| a.effect.as_deref() != Some("read"));
                if possible_mutation {
                    state.evidence.last_mutation_sequence = passive.sequence;
                }
                state.evidence.latest_stop_sequence = None;
                state.evidence.unresolved_attention = false;
                if agent
                    .and_then(|a| a.tool_id.as_ref())
                    .is_none_or(|id| !state.active_tools.remove(id))
                {
                    state.evidence.continuous_coverage = false;
                }
            }
            "attention-requested" => state.evidence.unresolved_attention = true,
            "stop-candidate" => state.evidence.latest_stop_sequence = Some(passive.sequence),
            "verified-run-started" | "verified-run-completed" => {
                if let Some(run) = agent.and_then(|a| a.verification.as_ref()) {
                    if run.session_id != wager.session_id
                        || Some(run.turn_id.as_str()) != wager.turn_id.as_deref()
                    {
                        state.evidence.attribution_valid = false;
                    }
                    if passive.event_type == "verified-run-started" {
                        if !state.active_runs.insert(run.run_id.clone()) {
                            state.evidence.continuous_coverage = false;
                        }
                        if state.evidence.runs.iter().any(|r| r.run_id == run.run_id) {
                            state.evidence.continuous_coverage = false;
                        } else {
                            state.evidence.runs.push(run.clone());
                        }
                    } else {
                        if !state.active_runs.remove(&run.run_id) {
                            state.evidence.continuous_coverage = false;
                        }
                        if let Some(prior) = state
                            .evidence
                            .runs
                            .iter_mut()
                            .find(|r| r.run_id == run.run_id)
                        {
                            if prior.started_sequence != run.started_sequence
                                || prior.suite_id != run.suite_id
                                || run.completed_sequence != passive.sequence
                            {
                                state.evidence.attribution_valid = false;
                            }
                            *prior = run.clone();
                        } else {
                            state.evidence.continuous_coverage = false;
                            state.evidence.runs.push(run.clone());
                        }
                    }
                } else {
                    state.evidence.continuous_coverage = false;
                }
            }
            _ => state.evidence.continuous_coverage = false,
        }
    }
    tx.execute(
        "UPDATE agent_evidence SET json=?2 WHERE wager_id=?1",
        params![wager.wager_id, encode(&state)?],
    )?;
    Ok(())
}
