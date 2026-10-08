//! TX module configuration over the CRSF parameter protocol, the same exchange the ExpressLRS
//! Lua script uses (design §3.1): ping → DEVICE_INFO, read every parameter (in chunks), write.
//!
//! Driven by frames from the module and a periodic `tick`; frames to send are collected in
//! an outbox that the TX loop sends in place of an RC frame (as EdgeTX does).

use elrs_crsf::{address, encode, parse_param_entry, DeviceInfo, Frame, ParamAssembler, ParamEntry, ParamType};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const PING_INTERVAL: Duration = Duration::from_millis(1000);
const READ_TIMEOUT: Duration = Duration::from_millis(300);
const READ_ATTEMPTS: u32 = 3;
const WRITE_TIMEOUT: Duration = Duration::from_millis(1000);
/// Delay before reading a written parameter back, giving the module time to apply it
const WRITE_SETTLE: Duration = Duration::from_millis(100);
/// No frame from the module for this long: considered disconnected
const MODULE_TIMEOUT: Duration = Duration::from_millis(3000);

#[derive(Debug, Clone, PartialEq)]
pub enum ParamEvent {
    /// The tree changed; send `params` to the UI
    TreeUpdated,
    WriteDone { request_id: u64, result: Result<(), String> },
}

#[derive(Debug)]
struct Read {
    number: u8,
    chunk: u8,
    attempts: u32,
    sent_at: Instant,
}

#[derive(Debug)]
struct Write {
    request_id: u64,
    number: u8,
    value: i32,
    started: Instant,
    reread_at: Option<Instant>,
}

pub struct ModuleConfig {
    device: Option<DeviceInfo>,
    params: BTreeMap<u8, ParamEntry>,
    assembler: ParamAssembler,
    read_queue: VecDeque<u8>,
    current_read: Option<Read>,
    write_queue: VecDeque<(u64, u8, i32)>,
    current_write: Option<Write>,
    last_ping: Option<Instant>,
    last_module_frame: Option<Instant>,
    outbox: VecDeque<Vec<u8>>,
    events: Vec<ParamEvent>,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleConfig {
    pub fn new() -> Self {
        Self {
            device: None,
            params: BTreeMap::new(),
            assembler: ParamAssembler::new(),
            read_queue: VecDeque::new(),
            current_read: None,
            write_queue: VecDeque::new(),
            current_write: None,
            last_ping: None,
            last_module_frame: None,
            outbox: VecDeque::new(),
            events: Vec::new(),
        }
    }

    pub fn device(&self) -> Option<&DeviceInfo> {
        self.device.as_ref()
    }

    pub fn params(&self) -> &BTreeMap<u8, ParamEntry> {
        &self.params
    }

    pub fn connected(&self, now: Instant) -> bool {
        self.last_module_frame
            .is_some_and(|at| now.saturating_duration_since(at) < MODULE_TIMEOUT)
    }

    /// The whole tree has been read
    pub fn complete(&self) -> bool {
        self.device.is_some() && self.read_queue.is_empty() && self.current_read.is_none()
    }

    pub fn take_outbox(&mut self) -> Vec<Vec<u8>> {
        self.outbox.drain(..).collect()
    }

    pub fn take_events(&mut self) -> Vec<ParamEvent> {
        std::mem::take(&mut self.events)
    }

    /// Re-reads the whole tree
    pub fn refresh(&mut self) {
        if let Some(device) = &self.device {
            self.read_queue = (1..=device.parameters_total).collect();
            self.current_read = None;
        } else {
            self.last_ping = None;
        }
    }

    /// Queues a write; the result arrives as [`ParamEvent::WriteDone`]
    pub fn write(&mut self, request_id: u64, number: u8, value: i32) -> Result<(), String> {
        let entry = self.params.get(&number).ok_or_else(|| format!("unknown parameter {number}"))?;
        match entry.kind {
            ParamType::Folder | ParamType::Info | ParamType::String | ParamType::OutOfRange => {
                return Err(format!("parameter {number} ({}) is not writable", entry.kind.name()))
            }
            ParamType::Command => {
                if !(0..=6).contains(&value) {
                    return Err("command status must be 0-6".into());
                }
            }
            _ => {
                if value < entry.raw.min || value > entry.raw.max {
                    return Err(format!(
                        "value {value} outside {}..={}",
                        entry.raw.min, entry.raw.max
                    ));
                }
            }
        }
        self.write_queue.push_back((request_id, number, value));
        Ok(())
    }

    pub fn on_frame(&mut self, frame: &Frame, now: Instant) {
        match frame {
            Frame::DeviceInfo(info) if info.origin == address::TX_MODULE => {
                self.last_module_frame = Some(now);
                let changed = self.device.as_ref() != Some(info);
                if changed {
                    log::info!(
                        "TX module: '{}' firmware {:#010x}, {} parameters",
                        info.name,
                        info.firmware_id,
                        info.parameters_total
                    );
                    self.device = Some(info.clone());
                    self.params.clear();
                    self.refresh();
                    self.events.push(ParamEvent::TreeUpdated);
                }
            }
            Frame::ParameterEntry(chunk) if chunk.origin == address::TX_MODULE => {
                self.last_module_frame = Some(now);
                self.on_chunk(chunk, now);
            }
            Frame::LinkStatistics(_) | Frame::OpenTxSync(_) | Frame::Battery(_) | Frame::FlightMode(_) => {
                self.last_module_frame = Some(now);
            }
            _ => {}
        }
    }

    fn on_chunk(&mut self, chunk: &elrs_crsf::ParamChunk, now: Instant) {
        let Some(read) = &mut self.current_read else { return };
        if chunk.number != read.number {
            return;
        }
        if let Some(data) = self.assembler.feed(chunk) {
            let number = read.number;
            self.current_read = None;
            match parse_param_entry(number, &data) {
                Some(entry) if entry.kind != ParamType::OutOfRange => {
                    if self.params.get(&number) != Some(&entry) {
                        self.params.insert(number, entry);
                        self.events.push(ParamEvent::TreeUpdated);
                    }
                }
                Some(_) => {}
                None => log::warn!("parameter {number}: undecodable entry"),
            }
            self.check_write_confirmed(number);
        } else if self.assembler.active() {
            // Next chunk (or chunk 0 again after an out-of-order chunk)
            read.chunk = self.assembler.next_chunk();
            read.attempts = 0;
            read.sent_at = now;
            self.outbox.push_back(encode::param_read(read.number, read.chunk));
        }
    }

    fn check_write_confirmed(&mut self, number: u8) {
        let Some(write) = &self.current_write else { return };
        if write.number != number || write.reread_at.is_some() {
            return;
        }
        let entry = self.params.get(&number);
        let result = match entry {
            // A command reports its new status; any answer confirms it was received
            Some(e) if e.kind == ParamType::Command => Ok(()),
            Some(e) if e.raw.value == write.value => Ok(()),
            Some(e) => Err(format!("module kept value {} (requested {})", e.raw.value, write.value)),
            None => Err("parameter vanished".into()),
        };
        self.events.push(ParamEvent::WriteDone { request_id: write.request_id, result });
        self.current_write = None;
    }

    pub fn tick(&mut self, now: Instant) {
        if self.device.is_none() || !self.connected(now) {
            if self.device.is_some() && !self.connected(now) {
                log::warn!("TX module silent; rediscovering");
                self.device = None;
                self.current_read = None;
                self.read_queue.clear();
            }
            if self.last_ping.is_none_or(|at| now.saturating_duration_since(at) >= PING_INTERVAL) {
                self.last_ping = Some(now);
                self.outbox.push_back(encode::device_ping());
            }
        }

        self.tick_write(now);
        self.tick_read(now);
    }

    fn tick_write(&mut self, now: Instant) {
        if let Some(write) = &mut self.current_write {
            if now.saturating_duration_since(write.started) > WRITE_TIMEOUT {
                self.events.push(ParamEvent::WriteDone {
                    request_id: write.request_id,
                    result: Err("no confirmation from the TX module".into()),
                });
                self.current_write = None;
            } else if write.reread_at.is_some_and(|at| now >= at) && self.current_read.is_none() {
                write.reread_at = None;
                let number = write.number;
                self.start_read(number, now);
            }
            return;
        }
        // Writes wait for an in-progress read so the re-read is not mixed with it
        if self.current_read.is_some() {
            return;
        }
        if let Some((request_id, number, value)) = self.write_queue.pop_front() {
            let Some(entry) = self.params.get(&number) else {
                self.events.push(ParamEvent::WriteDone {
                    request_id,
                    result: Err(format!("unknown parameter {number}")),
                });
                return;
            };
            let data = encode::param_value(entry.kind.raw(), value);
            self.outbox.push_back(encode::param_write(number, &data));
            self.current_write = Some(Write {
                request_id,
                number,
                value,
                started: now,
                reread_at: Some(now + WRITE_SETTLE),
            });
        }
    }

    fn start_read(&mut self, number: u8, now: Instant) {
        self.assembler.start(number);
        self.current_read = Some(Read { number, chunk: 0, attempts: 0, sent_at: now });
        self.outbox.push_back(encode::param_read(number, 0));
    }

    fn tick_read(&mut self, now: Instant) {
        if let Some(read) = &mut self.current_read {
            if now.saturating_duration_since(read.sent_at) < READ_TIMEOUT {
                return;
            }
            read.attempts += 1;
            if read.attempts < READ_ATTEMPTS {
                read.sent_at = now;
                self.outbox.push_back(encode::param_read(read.number, read.chunk));
                return;
            }
            log::warn!("parameter {}: no answer, skipped", read.number);
            self.current_read = None;
        }
        if self.device.is_some() && self.current_write.is_none() {
            if let Some(number) = self.read_queue.pop_front() {
                self.start_read(number, now);
            }
        }
    }
}

/// The tree as sent to the UI (docs/ipc.md `params`)
#[derive(Debug, Clone, Serialize)]
pub struct ParamView {
    pub number: u8,
    pub parent: u8,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: String,
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimal_point: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
}

impl From<&ParamEntry> for ParamView {
    fn from(entry: &ParamEntry) -> Self {
        let raw = &entry.raw;
        let mut view = ParamView {
            number: raw.number,
            parent: raw.parent,
            kind: entry.kind.name(),
            name: raw.name.clone(),
            hidden: raw.hidden,
            value: None,
            min: None,
            max: None,
            unit: None,
            options: None,
            decimal_point: None,
            step: None,
            text: None,
            children: None,
            status: None,
            info: None,
        };
        match entry.kind {
            ParamType::TextSelection => {
                view.value = Some(raw.value);
                view.min = Some(raw.min);
                view.max = Some(raw.max);
                view.options = Some(raw.options.clone());
                view.unit = Some(raw.unit.clone());
            }
            ParamType::String | ParamType::Info => view.text = Some(raw.text.clone()),
            ParamType::Folder => view.children = Some(raw.children.clone()),
            ParamType::Command => {
                view.status = Some(raw.status);
                view.info = Some(raw.text.clone());
            }
            ParamType::OutOfRange => {}
            _ => {
                view.value = Some(raw.value);
                view.min = Some(raw.min);
                view.max = Some(raw.max);
                view.unit = Some(raw.unit.clone());
                if entry.kind == ParamType::Float {
                    view.decimal_point = Some(raw.decimal_point);
                    view.step = Some(raw.step);
                }
            }
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_tools::fake_tx::ModuleModel;

    /// Runs a ModuleConfig against the fake module's parameter model, in-process
    fn exchange(config: &mut ModuleConfig, module: &mut ModuleModel, now: Instant) {
        for frame in config.take_outbox() {
            for reply in module.handle(&elrs_crsf::decode_bytes(&frame)) {
                config.on_frame(&elrs_crsf::decode_bytes(&reply), now);
            }
        }
    }

    fn run(config: &mut ModuleConfig, module: &mut ModuleModel, start: Instant, ms: u64) -> Instant {
        let mut now = start;
        for _ in 0..ms / 10 {
            now += Duration::from_millis(10);
            config.tick(now);
            exchange(config, module, now);
        }
        now
    }

    #[test]
    fn discovers_and_reads_the_whole_tree() {
        let mut module = ModuleModel::default();
        let mut config = ModuleConfig::new();
        let now = run(&mut config, &mut module, Instant::now(), 2000);
        assert!(config.complete());
        assert_eq!(config.device().unwrap().name, module.device_info().name);
        assert_eq!(config.params().len(), module.parameter_count());
        // The chunked entry was joined
        let rate = config.params().values().find(|p| p.raw.name == "Packet Rate").unwrap();
        assert!(rate.raw.options.len() >= 6);
        assert!(config.connected(now));
    }

    #[test]
    fn writes_and_confirms() {
        let mut module = ModuleModel::default();
        let mut config = ModuleConfig::new();
        let now = run(&mut config, &mut module, Instant::now(), 2000);
        let (number, power) = config
            .params()
            .iter()
            .find(|(_, p)| p.raw.name == "Max Power")
            .map(|(n, p)| (*n, p.raw.value))
            .unwrap();
        let target = if power == 0 { 1 } else { 0 };
        config.write(7, number, target).unwrap();
        run(&mut config, &mut module, now, 500);
        let events = config.take_events();
        assert!(events.contains(&ParamEvent::WriteDone { request_id: 7, result: Ok(()) }), "{events:?}");
        assert_eq!(config.params()[&number].raw.value, target);
        assert_eq!(module.value_of("Max Power"), Some(target));
    }

    #[test]
    fn rejects_bad_writes() {
        let mut module = ModuleModel::default();
        let mut config = ModuleConfig::new();
        run(&mut config, &mut module, Instant::now(), 2000);
        assert!(config.write(1, 200, 0).is_err());
        let folder = config.params().iter().find(|(_, p)| p.kind == ParamType::Folder).unwrap().0;
        assert!(config.write(1, *folder, 0).is_err());
        let power = config.params().iter().find(|(_, p)| p.raw.name == "Max Power").unwrap();
        let (number, max) = (*power.0, power.1.raw.max);
        assert!(config.write(1, number, max + 1).is_err());
    }

    #[test]
    fn write_times_out_without_module() {
        let mut module = ModuleModel::default();
        let mut config = ModuleConfig::new();
        let now = run(&mut config, &mut module, Instant::now(), 2000);
        let number = *config.params().iter().find(|(_, p)| p.raw.name == "Max Power").unwrap().0;
        config.write(3, number, 0).unwrap();
        let mut t = now;
        for _ in 0..150 {
            t += Duration::from_millis(10);
            config.tick(t);
            config.take_outbox(); // module unplugged: nothing answers
        }
        assert!(config
            .take_events()
            .iter()
            .any(|e| matches!(e, ParamEvent::WriteDone { request_id: 3, result: Err(_) })));
    }

    #[test]
    fn retries_lost_chunks() {
        let mut module = ModuleModel::default();
        module.drop_every_nth_reply(3);
        let mut config = ModuleConfig::new();
        run(&mut config, &mut module, Instant::now(), 8000);
        assert!(config.complete());
        assert_eq!(config.params().len(), module.parameter_count());
    }
}
