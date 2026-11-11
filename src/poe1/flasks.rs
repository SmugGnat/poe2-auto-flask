//! Bounded five-slot PoE1 flask reader. Charge-cost values are reported raw
//! until verified in-game with charge-usage modifiers; NO automation uses them.
use std::io;

use super::charge_cost;
use super::offsets::*;
use super::types::FlaskType;
use super::{invalid, plausible_ptr, read_metadata, read_ptr, Probe};
use crate::entity::resolve_component;
use crate::process::Process;

pub const SLOT_COUNT: usize = 5;
const MAX_INVENTORIES: usize = 256;
const MAX_CELLS: usize = 64;
const MAX_CHARGES: i32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlaskSlot {
    pub item_path: String,
    pub flask_type: FlaskType,
    pub current_charges: i32,
    pub raw_per_use: i32,
    pub effective_per_use: Option<i32>,
}

impl FlaskSlot {
    pub fn remaining_uses(&self) -> Option<i32> {
        let cost = self.effective_per_use?;
        (cost > 0).then_some(self.current_charges / cost)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlaskSnapshot {
    pub slots: [Option<FlaskSlot>; SLOT_COUNT],
}

#[derive(Debug, Clone)]
struct CachedItem {
    inventory_item: usize,
    entity: usize,
    charges_component: usize,
    item_path: String,
    local_charge_multiplier: Option<f64>,
}

#[derive(Debug, Default)]
pub struct FlaskCache {
    inventory: Option<usize>,
    slots: [Option<CachedItem>; SLOT_COUNT],
}

impl FlaskCache {
    pub fn clear(&mut self) {
        self.inventory = None;
        self.slots = std::array::from_fn(|_| None);
    }
}

pub fn sample(
    process: &Process,
    probe: &Probe,
    cache: &mut FlaskCache,
) -> io::Result<FlaskSnapshot> {
    // Validate that the player/ServerData chain still belongs to this session.
    let player_info = probe.area_instance + AREA_PLAYER_INFO;
    let pointers = process.read_bytes(player_info, 16)?;
    if u64_at(&pointers, 0) as usize != probe.server_data
        || u64_at(&pointers, 8) as usize != probe.local_player
    {
        cache.clear();
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "PoE1 PlayerInfo changed",
        ));
    }

    let (first, count) = read_vector(
        process,
        probe.server_data + SERVER_PLAYER_INVENTORIES,
        INVENTORY_DESCRIPTOR_STRIDE,
        MAX_INVENTORIES,
    )?;
    let mut flask_inventory = None;
    for index in 0..count {
        let at = first + index * INVENTORY_DESCRIPTOR_STRIDE;
        let descriptor = process.read_bytes(at, 16)?;
        let id = i32::from_le_bytes(descriptor[..4].try_into().expect("inventory ID"));
        if id == FLASK_INVENTORY_ID {
            let value = u64_at(&descriptor, 8) as usize;
            if !plausible_ptr(value) {
                return Err(invalid("invalid PoE1 flask inventory pointer"));
            }
            flask_inventory = Some(value);
            break;
        }
    }
    let inventory =
        flask_inventory.ok_or_else(|| invalid("PoE1 flask inventory ID 12 not found"))?;
    if cache.inventory != Some(inventory) {
        cache.clear();
        cache.inventory = Some(inventory);
    }

    let sizes = process.read_bytes(inventory + INVENTORY_SIZE, 8)?;
    let width = i32::from_le_bytes(sizes[..4].try_into().expect("width"));
    let height = i32::from_le_bytes(sizes[4..8].try_into().expect("height"));
    if width <= 0 || height <= 0 {
        return Err(invalid("invalid PoE1 flask inventory dimensions"));
    }
    let cells = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| invalid("PoE1 flask inventory size overflow"))?;
    if !(SLOT_COUNT..=MAX_CELLS).contains(&cells) {
        return Err(invalid("unexpected PoE1 flask inventory dimensions"));
    }

    let (items_first, item_count) =
        read_vector(process, inventory + INVENTORY_ITEMS, 8, MAX_CELLS)?;
    if item_count != cells {
        return Err(invalid(
            "PoE1 flask inventory dimensions and ItemList disagree",
        ));
    }
    let raw_slots = process.read_bytes(items_first, SLOT_COUNT * 8)?;
    // If Stats are temporarily unreadable, raw flask data remains available for
    // diagnostics but automation will not consider any cost to be trustworthy.
    let player_modifier = charge_cost::player_charge_modifier(process, probe.local_player).ok();
    let mut result = FlaskSnapshot {
        slots: std::array::from_fn(|_| None),
    };
    for slot in 0..SLOT_COUNT {
        let inventory_item = u64_at(&raw_slots, slot * 8) as usize;
        if inventory_item == 0 {
            cache.slots[slot] = None;
            continue;
        }
        if !plausible_ptr(inventory_item) {
            return Err(invalid(format!(
                "invalid inventory item at PoE1 slot {}",
                slot + 1
            )));
        }
        let entity = read_ptr(process, inventory_item)?;
        let existing = cache.slots[slot]
            .as_ref()
            .filter(|c| c.inventory_item == inventory_item && c.entity == entity);
        if existing.is_none() {
            let item_path = read_metadata(process, entity)?;
            if !item_path.starts_with("Metadata/Items/Flasks/") {
                return Err(invalid(format!(
                    "PoE1 slot {} is not a recognized flask: {item_path}",
                    slot + 1
                )));
            }
            let component = resolve_component(process, entity, "Charges")
                .ok_or_else(|| invalid(format!("PoE1 slot {} is missing Charges", slot + 1)))?;
            let local_charge_multiplier = charge_cost::local_charge_multiplier(process, entity).ok();
            cache.slots[slot] = Some(CachedItem {
                inventory_item,
                entity,
                charges_component: component,
                item_path,
                local_charge_multiplier,
            });
        }
        let cached = cache.slots[slot].as_ref().expect("item cached above");
        // Read the live field every time. It might already reflect charge modifiers.
        let charge_data = process.read_bytes(cached.charges_component + CHARGES_INTERNAL, 12)?;
        let internal = u64_at(&charge_data, 0) as usize;
        if !plausible_ptr(internal) {
            return Err(invalid("PoE1 Charges internal pointer invalid"));
        }
        let current = i32::from_le_bytes(
            charge_data[CHARGES_CURRENT - CHARGES_INTERNAL..CHARGES_CURRENT - CHARGES_INTERNAL + 4]
                .try_into()
                .expect("charges field"),
        );
        let per_use = process.read_i32(internal + CHARGES_PER_USE)?;
        if !(0..=MAX_CHARGES).contains(&current) || !(1..=MAX_CHARGES).contains(&per_use) {
            return Err(invalid(format!(
                "PoE1 slot {} has implausible charges {current}/{per_use}",
                slot + 1
            )));
        }
        let effective_per_use = player_modifier
            .zip(cached.local_charge_multiplier)
            .and_then(|(player, local)| charge_cost::effective_cost(per_use, player, local).ok());
        result.slots[slot] = Some(FlaskSlot {
            flask_type: FlaskType::from_metadata(&cached.item_path),
            item_path: cached.item_path.clone(),
            current_charges: current,
            raw_per_use: per_use,
            effective_per_use,
        });
    }
    Ok(result)
}

fn read_vector(
    process: &Process,
    at: usize,
    stride: usize,
    max_count: usize,
) -> io::Result<(usize, usize)> {
    let bytes = process.read_bytes(at, 16)?;
    let first = u64_at(&bytes, 0) as usize;
    let last = u64_at(&bytes, 8) as usize;
    let count = bounded_vector(first, last, stride, max_count)?;
    Ok((first, count))
}

fn bounded_vector(first: usize, last: usize, stride: usize, limit: usize) -> io::Result<usize> {
    if first == 0 && last == 0 {
        return Ok(0);
    }
    if !plausible_ptr(first) || last < first || stride == 0 {
        return Err(invalid("PoE1 vector pointers are invalid"));
    }
    let bytes = last - first;
    if bytes % stride != 0 || bytes / stride > limit {
        return Err(invalid("PoE1 vector length/stride is invalid"));
    }
    Ok(bytes / stride)
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("bounded pointer field"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_checks_reject_bad_ranges() {
        assert_eq!(bounded_vector(0, 0, 8, 5).unwrap(), 0);
        assert_eq!(bounded_vector(0x10000, 0x10028, 8, 5).unwrap(), 5);
        assert!(bounded_vector(0x10000, 0x10030, 8, 5).is_err());
        assert!(bounded_vector(0x10000, 0x10029, 8, 5).is_err());
        assert!(bounded_vector(0x20000, 0x10000, 8, 5).is_err());
        assert!(bounded_vector(0x10, 0x100, 8, 5).is_err());
    }

    #[test]
    fn cached_item_addresses_are_reset_on_area_change() {
        let mut cache = FlaskCache {
            inventory: Some(0x10000),
            ..FlaskCache::default()
        };
        cache.slots[0] = Some(CachedItem {
            inventory_item: 0x20000,
            entity: 0x30000,
            charges_component: 0x40000,
            item_path: "Metadata/Items/Flasks/Flask".into(),
            local_charge_multiplier: Some(1.0),
        });
        cache.clear();
        assert!(cache.inventory.is_none());
        assert!(cache.slots.iter().all(Option::is_none));
    }
    #[test]
    fn remaining_uses_requires_verified_effective_cost() {
        let mut slot = FlaskSlot {
            item_path: "Metadata/Items/Flasks/FlaskLife11".into(),
            flask_type: FlaskType::Life,
            current_charges: 50,
            raw_per_use: 15,
            effective_per_use: None,
        };
        assert_eq!(slot.remaining_uses(), None);
        slot.effective_per_use = Some(11);
        assert_eq!(slot.remaining_uses(), Some(4));
    }

}
