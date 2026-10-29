//! Device configuration model with lossless raw records.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::action::{Action, ActionKind};
use crate::protocol::codec::{self, CodecError};
use crate::protocol::read_config::ReadCapability;

pub const SCHEMA_VERSION: u32 = 1;
pub const RECORD_SIZE: usize = 50;
pub const LAYER_COUNT: usize = 3;
pub const MAX_POSITIONS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Validated,
    Experimental,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileIdentity {
    pub vid: u16,
    pub pid: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    pub key_count: u8,
    pub extra_count: u8,
    pub subtype: u8,
}

impl ProfileIdentity {
    pub fn tuple(&self) -> (u8, u8, u8) {
        (self.key_count, self.extra_count, self.subtype)
    }

    pub fn vid_pid_label(&self) -> String {
        format!("{:04x}:{:04x}", self.vid, self.pid)
    }

    pub fn backup_key(&self) -> String {
        self.serial
            .clone()
            .unwrap_or_else(|| format!("{:04x}-{:04x}", self.vid, self.pid))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigProvenance {
    Mock,
    ImportedProfile,
    VerifiedDeviceRead { capture_sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub schema_version: u32,
    pub identity: ProfileIdentity,
    pub layers: [LayerConfig; LAYER_COUNT],
    /// Per-layer RGB configuration uses the protocol's reserved record 0 and is
    /// not part of the ordinary physical-position records returned by `0xFA`.
    #[serde(default)]
    pub lighting: [Option<LightingConfig>; LAYER_COUNT],
    #[serde(default)]
    pub opaque_device_data: Vec<u8>,
    pub provenance: ConfigProvenance,
}

impl DeviceConfig {
    pub fn empty_for(identity: ProfileIdentity, provenance: ConfigProvenance) -> Self {
        let positions_hint = (identity.key_count as usize)
            .saturating_add(identity.extra_count as usize)
            .clamp(1, MAX_POSITIONS);
        let layer = LayerConfig {
            positions: (0..positions_hint as u8)
                .map(PositionConfig::empty)
                .collect(),
        };
        Self {
            schema_version: SCHEMA_VERSION,
            identity,
            layers: [layer.clone(), layer.clone(), layer],
            lighting: [None, None, None],
            opaque_device_data: Vec::new(),
            provenance,
        }
    }

    pub fn dirty_count(&self) -> usize {
        let positions = self
            .layers
            .iter()
            .map(|l| l.positions.iter().filter(|p| p.dirty).count())
            .sum::<usize>();
        positions + self.lighting.iter().flatten().filter(|l| l.dirty).count()
    }

    pub fn dirty_action_kinds(&self) -> std::collections::BTreeSet<ActionKind> {
        let mut set = std::collections::BTreeSet::new();
        for layer in &self.layers {
            for pos in &layer.positions {
                if pos.dirty {
                    set.insert(pos.action.kind());
                }
            }
        }
        if self
            .lighting
            .iter()
            .flatten()
            .any(|lighting| lighting.dirty)
        {
            set.insert(ActionKind::Lighting);
        }
        set
    }

    pub fn clear_dirty(&mut self) {
        for layer in &mut self.layers {
            for pos in &mut layer.positions {
                pos.dirty = false;
            }
        }
        for lighting in self.lighting.iter_mut().flatten() {
            lighting.dirty = false;
        }
    }

    pub fn apply_lighting(
        &mut self,
        layer: usize,
        mode: u8,
        color: u8,
        verified: &std::collections::BTreeSet<ActionKind>,
    ) -> Result<(), CodecError> {
        if !verified.contains(&ActionKind::Lighting) {
            return Err(CodecError::UnverifiedAction(ActionKind::Lighting));
        }
        let raw_record = codec::encode_lighting_record(layer, mode, color)?;
        let previous = self.lighting.get(layer).ok_or_else(|| {
            CodecError::Message(format!("lighting layer {layer} outside 0..{LAYER_COUNT}"))
        })?;
        let dirty = previous
            .as_ref()
            .is_none_or(|lighting| lighting.raw_record != raw_record);
        self.lighting[layer] = Some(LightingConfig {
            mode,
            color,
            raw_record,
            dirty,
        });
        Ok(())
    }

    pub fn is_writable(
        &self,
        support: SupportLevel,
        read_capability: &ReadCapability,
        verified_actions: &std::collections::BTreeSet<ActionKind>,
    ) -> bool {
        if support != SupportLevel::Validated {
            return false;
        }
        if !matches!(read_capability, ReadCapability::Verified { .. }) {
            return false;
        }
        match &self.provenance {
            ConfigProvenance::VerifiedDeviceRead { .. } | ConfigProvenance::ImportedProfile => {}
            ConfigProvenance::Mock => return false,
        }
        self.dirty_action_kinds()
            .iter()
            .all(|kind| verified_actions.contains(kind))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingConfig {
    pub mode: u8,
    pub color: u8,
    #[serde(
        serialize_with = "serialize_raw_record",
        deserialize_with = "deserialize_raw_record"
    )]
    pub raw_record: [u8; RECORD_SIZE],
    #[serde(skip, default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerConfig {
    pub positions: Vec<PositionConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionConfig {
    pub logical_index: u8,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u16>,
    #[serde(
        serialize_with = "serialize_raw_record",
        deserialize_with = "deserialize_raw_record"
    )]
    pub raw_record: [u8; RECORD_SIZE],
    #[serde(skip, default)]
    pub dirty: bool,
}

impl PositionConfig {
    pub fn empty(logical_index: u8) -> Self {
        Self {
            logical_index,
            action: Action::Empty,
            delay_ms: None,
            raw_record: [0u8; RECORD_SIZE],
            dirty: false,
        }
    }

    /// Encode `action` over a clone of `raw_record`, update semantics, mark dirty only on byte change.
    pub fn apply_action(
        &mut self,
        action: Action,
        delay_ms: Option<u16>,
        verified: &std::collections::BTreeSet<ActionKind>,
    ) -> Result<(), CodecError> {
        if matches!(action, Action::Opaque { .. }) {
            return Err(CodecError::UnsupportedOpaqueEdit);
        }
        let new_raw = codec::encode_record(self.raw_record, &action, delay_ms, verified)?;
        let changed = new_raw != self.raw_record;
        self.raw_record = new_raw;
        self.action = action;
        self.delay_ms = delay_ms;
        if changed {
            self.dirty = true;
        }
        Ok(())
    }
}

fn serialize_raw_record<S>(raw: &[u8; RECORD_SIZE], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&hex::encode(raw))
}

fn deserialize_raw_record<'de, D>(deserializer: D) -> Result<[u8; RECORD_SIZE], D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
    if bytes.len() != RECORD_SIZE {
        return Err(serde::de::Error::custom(format!(
            "raw_record must be {RECORD_SIZE} bytes ({} hex chars), got {} bytes",
            RECORD_SIZE * 2,
            bytes.len()
        )));
    }
    let mut arr = [0u8; RECORD_SIZE];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn raw_record_hex_roundtrip() {
        let mut pos = PositionConfig::empty(0);
        pos.raw_record[0] = 0xFE;
        pos.raw_record[1] = 0xB0;
        pos.raw_record[9] = 1;
        let json = serde_json::to_string(&pos).unwrap();
        assert!(json.contains("\"raw_record\":\"feb0"));
        let back: PositionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.raw_record, pos.raw_record);
        assert!(!back.dirty);
    }

    #[test]
    fn apply_action_rejects_opaque() {
        let mut pos = PositionConfig::empty(0);
        let err = pos
            .apply_action(Action::Opaque { mode: 1 }, None, &BTreeSet::new())
            .unwrap_err();
        assert!(matches!(err, CodecError::UnsupportedOpaqueEdit));
    }

    #[test]
    fn lighting_does_not_replace_a_physical_position() {
        let identity = ProfileIdentity {
            vid: 0x1189,
            pid: 0x8842,
            serial: None,
            key_count: 1,
            extra_count: 0,
            subtype: 0,
        };
        let mut config = DeviceConfig::empty_for(identity, ConfigProvenance::Mock);
        let original = config.layers[0].positions[0].clone();
        let verified = BTreeSet::from([ActionKind::Lighting]);

        config.apply_lighting(0, 2, 3, &verified).unwrap();

        assert_eq!(config.layers[0].positions[0], original);
        assert_eq!(config.dirty_count(), 1);
        let lighting = config.lighting[0].as_ref().unwrap();
        assert_eq!(&lighting.raw_record[..4], &[0xFE, 0xB0, 1, 8]);
        assert_eq!(lighting.raw_record[11], 0x23);
    }
}
