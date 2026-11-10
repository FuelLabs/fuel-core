//! Opt-in Ethereum confirmation, with direct-finalized fallback and durable exposure evidence.

use crate::{
    config::{
        ETH_FORCED_TX,
        ETH_LOG_MESSAGE,
    },
    ports::RelayerDb,
};
use alloy_primitives::{
    B256,
    keccak256,
};
use alloy_rpc_types_eth::Log;
use anyhow::{
    Result,
    ensure,
};
use fuel_core_types::services::relayer::Event;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Value,
    json,
};
use std::{
    fs::{
        self,
        File,
        OpenOptions,
    },
    io::Write,
    path::PathBuf,
    time::{
        Duration,
        SystemTime,
        UNIX_EPOCH,
    },
};
use url::Url;

/// Configuration of a provider-local FCR view. All direct and wrapped routes must agree.
#[derive(Clone, Debug)]
pub struct Config {
    /// Wrapped HTTP execution routes, paired in order with `relayer` routes.
    pub rpc: Vec<Url>,
    /// Persistent safety state on the node's database volume.
    pub state_path: PathBuf,
    /// Expected Ethereum chain ID (1 or 11155111).
    pub chain_id: u64,
    /// Recovery generation, changed only by coordinated offline recovery.
    pub generation: String,
    /// Maximum not-yet-finalized block exposure and ancestry replay span.
    pub max_unchecked_blocks: u64,
    /// Keep the checker and activation marker while disabling new early ingestion.
    pub finalized_only: bool,
    /// Maximum time before freezing additional early exposure (checking continues).
    pub max_unchecked_seconds: u64,
}

/// Exact block identity used for consumption and eventual-finalized comparison.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// Ethereum execution height.
    pub number: u64,
    /// Ethereum execution hash.
    pub hash: B256,
    /// Parent identity, omitted only in the wrapper's minimal response.
    pub parent_hash: Option<B256>,
}

/// Stored atomically with a DA height's events. Finalized catch-up needs only a range-end header.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockEvidence {
    /// Full header for early blocks and finalized range endpoints.
    pub header: Option<Header>,
    /// Agreed complete relevant log vector, including empty vectors.
    pub logs_digest: B256,
    /// Whether consumption preceded independent Ethereum finality.
    pub early: bool,
    /// Admission time for bounding the oldest outstanding early exposure.
    pub admitted_at: u64,
}

/// Read-only safety information. It contains no provider URLs or credentials.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    /// Whether the node has enabled early ingestion.
    pub enabled: bool,
    /// Normal, degraded, held or incident.
    pub state: String,
    /// Recovery generation, distinct from provider membership.
    pub generation: String,
    /// Highest consumed DA height checked against direct finality.
    pub checked_height: u64,
    /// Unix time of the last successful independent finalized check.
    pub checked_at: u64,
    /// Corroborated mismatch identifier; presence is terminal until explicit recovery.
    pub incident: Option<String>,
    /// Why early admission is capped while independent finalized checks continue.
    pub admission_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    chain_id: u64,
    generation: String,
    contracts: Vec<alloy_primitives::Address>,
    checked: Header,
    checked_at: u64,
    incident: Option<String>,
    #[serde(default)]
    recoveries: Vec<Recovery>,
}

/// A per-node driver. The lock remains held for the lifetime of the service.
pub(crate) struct Driver {
    config: Config,
    direct: Vec<Url>,
    client: reqwest::Client,
    state: Option<State>,
    _lock: File,
    latch: IncidentLatch,
    reporter: Option<std::sync::Arc<dyn HaltReporter>>,
    admission_reason: Option<String>,
    volatile_incident: Option<String>,
}

fn quantity(value: &Value) -> Result<u64> {
    let text = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing Ethereum quantity"))?;
    let hex = text
        .strip_prefix("0x")
        .ok_or_else(|| anyhow::anyhow!("Invalid Ethereum quantity"))?;
    ensure!(
        !hex.is_empty() && (hex == "0" || !hex.starts_with('0')),
        "Noncanonical Ethereum quantity"
    );
    Ok(u64::from_str_radix(hex, 16)?)
}

fn header(value: Value) -> Result<Header> {
    Ok(Header {
        number: quantity(&value["number"])?,
        hash: value["hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing Ethereum hash"))?
            .parse()?,
        parent_hash: value["parentHash"].as_str().map(str::parse).transpose()?,
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn normalized(logs: &mut [Log]) -> Result<Vec<Value>> {
    logs.sort_by_key(|log| (log.block_number, log.transaction_index, log.log_index));
    let mut previous = None;
    let mut values = Vec::with_capacity(logs.len());
    for log in logs {
        ensure!(
            !log.removed
                && log.block_number.is_some()
                && log.block_hash.is_some()
                && log.transaction_hash.is_some()
                && log.transaction_index.is_some()
                && log.log_index.is_some(),
            "Unbound Ethereum log"
        );
        let position = (log.block_number, log.log_index);
        ensure!(previous != Some(position), "Duplicate Ethereum log index");
        previous = Some(position);
        values.push(json!([
            log.block_number,
            log.block_hash,
            log.transaction_hash,
            log.transaction_index,
            log.log_index,
            log.address(),
            log.topics(),
            log.data().data
        ]));
    }
    Ok(values)
}

fn digest(logs: &[Log]) -> Result<B256> {
    let mut logs = logs.to_vec();
    Ok(keccak256(serde_json::to_vec(&normalized(&mut logs)?)?))
}

impl Driver {
    pub(crate) fn open(
        config: Config,
        direct: Vec<Url>,
        contracts: &[alloy_primitives::Address],
        reporter: Option<std::sync::Arc<dyn HaltReporter>>,
    ) -> Result<Self> {
        ensure!(
            matches!(config.chain_id, 1 | 11155111),
            "FCR only supports Ethereum Mainnet/Sepolia"
        );
        ensure!(
            !direct.is_empty() && direct.len() == config.rpc.len(),
            "FCR requires one wrapped route per direct provider"
        );
        ensure!(
            !config.generation.is_empty()
                && config.max_unchecked_blocks > 0
                && config.max_unchecked_seconds > 0,
            "Invalid FCR exposure configuration"
        );
        for routes in [&direct, &config.rpc] {
            let distinct = routes.iter().collect::<std::collections::HashSet<_>>();
            ensure!(distinct.len() == routes.len(), "Duplicate FCR route");
            ensure!(
                routes
                    .iter()
                    .all(|url| matches!(url.scheme(), "http" | "https")),
                "FCR requires HTTP routes"
            );
        }
        let parent = config
            .state_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("FCR state needs an explicit parent directory")
            })?;
        fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(config.state_path.with_extension("lock"))?;
        lock.try_lock()
            .map_err(|_| anyhow::anyhow!("FCR state is already in use"))?;
        let state: Option<State> = match fs::read(&config.state_path) {
            Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(state) = &state {
            ensure!(
                state.chain_id == config.chain_id
                    && state.generation == config.generation
                    && state.contracts == contracts,
                "FCR state identity changed; explicit offline recovery required"
            );
        }
        let latch = IncidentLatch::new(&config.state_path);
        Ok(Self {
            latch,
            reporter,
            admission_reason: None,
            volatile_incident: None,
            config,
            direct,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
            state,
            _lock: lock,
        })
    }

    pub(crate) fn status(&self, phase: &str) -> Status {
        let incident =
            self.volatile_incident
                .clone()
                .or_else(|| match self.latch.read() {
                    Ok(value) => value.or_else(|| {
                        self.state.as_ref().and_then(|state| state.incident.clone())
                    }),
                    Err(_) => Some("unreadable-incident-latch".into()),
                });
        Status {
            enabled: true,
            state: if incident.is_some() {
                "incident".into()
            } else {
                phase.into()
            },
            generation: self.config.generation.clone(),
            checked_height: self.state.as_ref().map_or(0, |state| state.checked.number),
            checked_at: self.state.as_ref().map_or(0, |state| state.checked_at),
            incident,
            admission_reason: self.admission_reason.clone(),
        }
    }

    fn declare(&mut self, incident: &str) -> Result<()> {
        let retained = self
            .volatile_incident
            .clone()
            .or_else(|| self.state.as_ref().and_then(|state| state.incident.clone()))
            .or_else(|| self.latch.read().ok().flatten())
            .unwrap_or_else(|| incident.into());
        let incident = retained.as_str();
        // A failed disk write still closes the live process gate immediately.
        if self.volatile_incident.as_deref() != Some(incident) {
            tracing::error!(
                incident,
                "Canonical FCR incident: production/import fenced; explicit recovery required"
            );
        }
        self.volatile_incident = Some(incident.into());
        if let Some(state) = &mut self.state {
            state.incident = Some(incident.into());
        }
        self.latch.declare(incident)?;
        if self.state.is_some() {
            self.save()?;
        }
        Ok(())
    }

    pub(crate) async fn propagate(&mut self) -> Result<()> {
        if let Some(reporter) = self.reporter.clone() {
            let observed = reporter.observed().await;
            let mut persistence = Ok(());
            if let Ok(Some(incident)) = &observed {
                persistence = self.declare(incident);
            }
            // Continue reporting a known positive even when observation/persistence is
            // temporarily unavailable. Never terminate the incident reporter on quorum loss.
            if let Some(incident) = self.status("normal").incident {
                reporter.report(&incident).await?;
            }
            observed?;
            persistence?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let bytes = serde_json::to_vec(
            self.state
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("FCR activation is incomplete"))?,
        )?;
        let temporary = self.config.state_path.with_extension("tmp");
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(temporary, &self.config.state_path)?;
        if let Some(parent) = self.config.state_path.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    }

    async fn rpc(&self, url: &Url, method: &str, params: Value) -> Result<Value> {
        let response = self
            .client
            .post(url.clone())
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Ethereum RPC unavailable"))?;
        ensure!(response.status().is_success(), "Ethereum RPC HTTP failure");
        let value: Value = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("Invalid Ethereum RPC response"))?;
        ensure!(
            value["id"] == 1 && value.get("error").is_none(),
            "Ethereum RPC error"
        );
        value
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing Ethereum RPC result"))
    }

    async fn block(&self, url: &Url, number: u64) -> Result<Header> {
        let block = header(
            self.rpc(
                url,
                "eth_getBlockByNumber",
                json!([format!("0x{number:x}"), false]),
            )
            .await?,
        )?;
        ensure!(block.number == number, "Ethereum block height mismatch");
        Ok(block)
    }

    async fn common_block(&self, number: u64) -> Result<Header> {
        let mut result = None;
        for url in &self.direct {
            let block = self.block(url, number).await?;
            if let Some(previous) = &result {
                ensure!(previous == &block, "Ethereum block disagreement");
            }
            result = Some(block);
        }
        result.ok_or_else(|| anyhow::anyhow!("No Ethereum providers"))
    }

    async fn common_head(&self, urls: &[Url], tag: &str) -> Result<Header> {
        let mut heads = Vec::new();
        for url in urls {
            ensure!(
                quantity(&self.rpc(url, "eth_chainId", json!([])).await?)?
                    == self.config.chain_id,
                "Ethereum chain ID mismatch"
            );
            heads.push(header(
                self.rpc(url, "eth_getBlockByNumber", json!([tag, false]))
                    .await?,
            )?);
        }
        let height = heads
            .iter()
            .map(|head| head.number)
            .min()
            .ok_or_else(|| anyhow::anyhow!("No Ethereum heads"))?;
        let mut common = None;
        for (url, head) in urls.iter().zip(heads) {
            ensure!(
                head.number.saturating_sub(height) <= self.config.max_unchecked_blocks,
                "Ethereum head gap exceeds bound"
            );
            let first = self.block(url, height).await?;
            if let Some(previous) = &common {
                ensure!(previous == &first, "Ethereum head disagreement");
            }
            let mut previous = first.clone();
            for number in height.saturating_add(1)..=head.number {
                let next = self.block(url, number).await?;
                ensure!(
                    next.parent_hash == Some(previous.hash),
                    "Ethereum head ancestry mismatch"
                );
                previous = next;
            }
            ensure!(previous.hash == head.hash, "Ethereum head changed branch");
            common = Some(first);
        }
        common.ok_or_else(|| anyhow::anyhow!("No Ethereum head"))
    }

    async fn logs(
        &self,
        contracts: &[alloy_primitives::Address],
        filter: Value,
    ) -> Result<Vec<Log>> {
        let mut result: Option<Vec<Log>> = None;
        let mut expected = None;
        for url in &self.direct {
            let mut filter = filter.clone();
            filter["address"] = json!(contracts);
            filter["topics"] = json!([[*ETH_LOG_MESSAGE, *ETH_FORCED_TX]]);
            let mut logs: Vec<Log> = serde_json::from_value(
                self.rpc(url, "eth_getLogs", json!([filter])).await?,
            )?;
            let normalized = normalized(&mut logs)?;
            for log in &logs {
                ensure!(
                    contracts.contains(&log.address())
                        && matches!(log.topics().first(), Some(topic) if *topic == *ETH_LOG_MESSAGE || *topic == *ETH_FORCED_TX),
                    "Unexpected Ethereum event"
                );
            }
            if let Some(previous) = &expected {
                ensure!(previous == &normalized, "Ethereum log-set disagreement");
            }
            expected = Some(normalized);
            result = Some(logs);
        }
        result.ok_or_else(|| anyhow::anyhow!("No Ethereum logs"))
    }

    async fn block_logs(
        &self,
        contracts: &[alloy_primitives::Address],
        block: &Header,
    ) -> Result<Vec<Log>> {
        let logs = self
            .logs(contracts, json!({"blockHash": block.hash}))
            .await?;
        ensure!(
            logs.iter().all(|log| log.block_hash == Some(block.hash)
                && log.block_number == Some(block.number)),
            "Ethereum log block mismatch"
        );
        Ok(logs)
    }

    pub(crate) async fn sync<D: RelayerDb>(
        &mut self,
        db: &mut D,
        config: &crate::Config,
    ) -> Result<u64> {
        if self.status("normal").incident.is_some() {
            return Ok(db.get_finalized_da_height().map_or(0, |height| height.0));
        }
        let finalized = self.common_head(&self.direct, "finalized").await?;
        let local = db
            .get_finalized_da_height()
            .map_or(config.da_deploy_height.0.saturating_sub(1), |height| {
                height.0
            });
        if self.state.is_none() {
            ensure!(
                !db.fcr_activated()?,
                "Activated FCR safety state is missing"
            );
            ensure!(
                local <= finalized.number,
                "FCR activation requires a finalized local anchor"
            );
            let checked = self.common_block(local).await?;
            self.state = Some(State {
                chain_id: self.config.chain_id,
                generation: self.config.generation.clone(),
                contracts: config.eth_v2_listening_contracts.clone(),
                checked,
                checked_at: now(),
                incident: None,
                recoveries: Vec::new(),
            });
            self.save()?;
            db.activate_fcr(local.into())?;
        } else {
            db.activate_fcr(local.into())?;
        }
        let checked = self.state.as_ref().expect("activated").checked.number;
        ensure!(
            checked <= local,
            "Relayer rewind requires explicit FCR recovery"
        );
        ensure!(
            finalized.number >= checked,
            "Finalized providers are behind the retained checked anchor"
        );
        let retained = self.common_block(checked).await?;
        if retained.hash != self.state.as_ref().expect("activated").checked.hash {
            ensure!(
                self.direct.len() >= 2,
                "Single-route finalized contradiction remains held"
            );
            self.declare(&format!(
                "finalized-anchor:{}:{checked}:{:?}",
                self.config.chain_id, retained.hash
            ))?;
            return Ok(local);
        }
        let end = local.min(finalized.number);
        for number in checked.saturating_add(1)..=end {
            if let Some(evidence) = db.fcr_evidence(&number.into())? {
                if evidence.early {
                    let block = self.common_block(number).await?;
                    let logs = self
                        .block_logs(&config.eth_v2_listening_contracts, &block)
                        .await?;
                    if evidence.header.as_ref().map(|header| header.hash)
                        != Some(block.hash)
                        || evidence.logs_digest != digest(&logs)?
                    {
                        let id = format!(
                            "{}:{}:{number}:{:?}",
                            self.config.chain_id, self.config.generation, block.hash
                        );
                        ensure!(
                            self.direct.len() >= 2,
                            "Single-route finalized contradiction remains held"
                        );
                        self.declare(&id)?;
                        return Ok(local);
                    }
                }
            } else {
                anyhow::bail!("FCR event evidence is missing");
            }
        }
        if end > checked {
            self.state.as_mut().expect("activated").checked =
                self.common_block(end).await?;
        }
        self.state.as_mut().expect("activated").checked_at = now();
        self.save()?;
        if local < finalized.number {
            let end = local
                .saturating_add(config.log_page_size.max(1))
                .min(finalized.number);
            let all_logs = self.logs(&config.eth_v2_listening_contracts, json!({"fromBlock":format!("0x{:x}", local.saturating_add(1)),"toBlock":format!("0x{end:x}")})).await?;
            ensure!(all_logs.iter().all(|log| matches!(log.block_number, Some(number) if number > local && number <= end)), "Ethereum range log outside requested interval");
            let anchor = self.common_block(end).await?;
            for number in local.saturating_add(1)..=end {
                let logs: Vec<_> = all_logs
                    .iter()
                    .filter(|log| log.block_number == Some(number))
                    .cloned()
                    .collect();
                let events = decode(logs.clone())?;
                let evidence = BlockEvidence {
                    header: (number == end).then(|| anchor.clone()),
                    logs_digest: digest(&logs)?,
                    early: false,
                    admitted_at: now(),
                };
                db.insert_fcr_events(&number.into(), &events, &evidence)?;
            }
            self.state.as_mut().expect("activated").checked = anchor;
            self.save()?;
            return Ok(finalized.number);
        }
        self.admission_reason = None;
        if self.config.finalized_only {
            self.admission_reason = Some("finalized-only".into());
            return Ok(local);
        }
        // Revalidate every outstanding block before widening the prefix. A new route
        // policy cannot silently replace old unchecked branch/log evidence.
        let checked = self.state.as_ref().expect("activated").checked.number;
        for number in checked.saturating_add(1)..=local {
            let evidence = db
                .fcr_evidence(&number.into())?
                .ok_or_else(|| anyhow::anyhow!("Missing unchecked FCR evidence"))?;
            let block = self.common_block(number).await?;
            let logs = self
                .block_logs(&config.eth_v2_listening_contracts, &block)
                .await?;
            ensure!(
                evidence.header.as_ref().map(|header| header.hash) == Some(block.hash)
                    && evidence.logs_digest == digest(&logs)?,
                "Unchecked Ethereum branch/data changed"
            );
            if now().saturating_sub(evidence.admitted_at)
                >= self.config.max_unchecked_seconds
            {
                self.admission_reason = Some("unchecked-time-budget".into());
                return Ok(local);
            }
        }
        let confirmed = match self.common_head(&self.config.rpc, "confirmed").await {
            Ok(confirmed)
                if confirmed.number > local
                    && confirmed.number
                        <= finalized
                            .number
                            .saturating_add(self.config.max_unchecked_blocks) =>
            {
                confirmed
            }
            _ => {
                self.admission_reason =
                    Some("confirmed-unavailable-or-exposure-bound".into());
                return Ok(local);
            }
        };
        let target = confirmed.number;
        let mut previous = self.common_block(local).await?;
        if let Some(evidence) = db.fcr_evidence(&local.into())? {
            if let Some(stored) = evidence.header {
                ensure!(
                    stored.hash == previous.hash,
                    "Consumed Ethereum branch changed before finality"
                );
            }
        } else {
            ensure!(
                local <= self.state.as_ref().expect("activated").checked.number,
                "Missing early anchor"
            );
        }
        let mut headers = Vec::new();
        for number in local.saturating_add(1)..=target {
            let block = self.common_block(number).await?;
            ensure!(
                block.parent_hash == Some(previous.hash),
                "Ethereum parent continuity mismatch"
            );
            previous = block.clone();
            headers.push(block);
        }
        ensure!(
            previous.hash == confirmed.hash,
            "Ethereum early prefix is not confirmed"
        );
        for block in headers {
            let number = block.number;
            let logs = self
                .block_logs(&config.eth_v2_listening_contracts, &block)
                .await?;
            if logs
                .iter()
                .any(|log| log.topics().first() == Some(&*ETH_FORCED_TX))
            {
                self.admission_reason = Some("unfinalized-forced-transaction".into());
                return Ok(number.saturating_sub(1));
            }
            let tip = self.common_block(confirmed.number).await?;
            ensure!(
                tip.hash == confirmed.hash,
                "Ethereum confirmation changed branch"
            );
            let rechecked = self.common_block(number).await?;
            ensure!(
                rechecked.hash == block.hash,
                "Ethereum block changed during admission"
            );
            let events = decode(logs.clone())?;
            let evidence = BlockEvidence {
                header: Some(block.clone()),
                logs_digest: digest(&logs)?,
                early: true,
                admitted_at: now(),
            };
            db.insert_fcr_events(&number.into(), &events, &evidence)?;
        }
        Ok(target)
    }
}

fn decode(logs: Vec<Log>) -> Result<Vec<Event>> {
    use crate::log::EthEventLog;
    let mut events = Vec::new();
    for log in logs {
        match EthEventLog::try_from(&log)? {
            EthEventLog::Message(message) => {
                events.push(Event::Message((&message).into()))
            }
            EthEventLog::Transaction(transaction) => {
                events.push(Event::Transaction(transaction.into()))
            }
            EthEventLog::Ignored => {}
        }
    }
    Ok(events)
}

/// Immutable positive incident tombstone, separate from cursor updates and DA rollback.
#[derive(Clone, Debug)]
pub struct IncidentLatch(PathBuf);

impl IncidentLatch {
    /// Use the tombstone beside the configured safety file on the same persistent volume.
    pub fn new(state: &std::path::Path) -> Self {
        Self(state.with_extension("incident"))
    }

    /// Read a previously observed incident. A partial/unreadable file is never a clean state.
    pub fn read(&self) -> Result<Option<String>> {
        match fs::read_to_string(&self.0) {
            Ok(value) => {
                ensure!(!value.trim().is_empty(), "Incomplete FCR incident latch");
                Ok(Some(value))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Persist a positive signal before permitting any subsequent production.
    pub fn declare(&self, incident: &str) -> Result<()> {
        ensure!(
            !incident.is_empty() && incident.len() <= 512,
            "Invalid FCR incident identifier"
        );
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&self.0) {
            Ok(mut file) => {
                file.write_all(incident.as_bytes())?;
                file.sync_all()?;
                if let Some(parent) = self.0.parent() {
                    File::open(parent)?.sync_all()?;
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                self.read()?;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

/// Existing authority infrastructure supplies the positive Redis fence; followers use no reporter.
pub trait HaltReporter: std::fmt::Debug + Send + Sync {
    /// Observe any positive peer signal, copying it to this node's durable latch.
    fn observed(&self) -> futures::future::BoxFuture<'_, Result<Option<String>>>;
    /// Persist a halt on the existing strict production quorum; failure must be retried.
    fn report<'a>(
        &'a self,
        incident: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<()>>;
}

#[cfg(test)]
mod tests;

/// Reviewed offline recovery receipt. It does not authorize or perform network recovery.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recovery {
    /// Exact positive local latch being acknowledged.
    pub incident: String,
    /// Previous safety generation.
    pub previous_generation: String,
    /// New authority recovery generation.
    pub generation: String,
    /// Independently verified finalized checkpoint after DA rollback.
    pub checked: Header,
    /// Operator's record of Fuel/DA rollback, Redis quorum reset, signer invalidation
    /// and isolation/resynchronization of old signed P2P history.
    pub recovery_record: String,
}

/// Re-anchor an already activated journal after coordinated recovery, preserving its history.
/// The caller must hold the node database exclusively and verify the actual DA checkpoint.
pub fn recover_offline(
    path: &std::path::Path,
    recovery: Recovery,
    actual_da_height: u64,
) -> Result<()> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    lock.try_lock()
        .map_err(|_| anyhow::anyhow!("Stop the node before FCR recovery"))?;
    let mut state: State = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        actual_da_height == recovery.checked.number,
        "Rollback DA height does not match the recovery checkpoint"
    );
    ensure!(
        !recovery.recovery_record.trim().is_empty()
            && !recovery.generation.is_empty()
            && recovery.generation != recovery.previous_generation,
        "Recovery requires a new generation and an operational recovery record"
    );
    let latch = IncidentLatch::new(path);
    ensure!(
        latch.read()?.as_deref() == Some(recovery.incident.as_str()),
        "Acknowledge the exact active incident"
    );
    let resumed = state.generation == recovery.generation
        && state.recoveries.last().is_some_and(|previous| {
            previous.incident == recovery.incident && previous.checked == recovery.checked
        });
    ensure!(
        resumed || state.generation == recovery.previous_generation,
        "Recovery generation does not match retained state"
    );
    if !resumed {
        state.generation = recovery.generation.clone();
        state.checked = recovery.checked.clone();
        state.checked_at = 0;
        state.incident = None;
        state.recoveries.push(recovery.clone());
        let temporary = path.with_extension("tmp");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(&state)?)?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        File::open(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Missing recovery parent"))?,
        )?
        .sync_all()?;
    }
    // Do this last: a crash before here leaves a positive halt, never a clean partial recovery.
    let archived = path.with_extension(format!("incident.{}", state.recoveries.len()));
    ensure!(
        !archived.exists(),
        "Recovery incident archive already exists"
    );
    fs::rename(&latch.0, archived)?;
    File::open(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Missing recovery parent"))?,
    )?
    .sync_all()?;
    Ok(())
}
