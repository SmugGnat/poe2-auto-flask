use std::io;

use crate::entity::resolve_component;
use crate::poe2::VitalsProbe;
use crate::process::Process;

const PLAYER_INFO_LOCAL_PLAYER: usize = 0x20;
const SERVER_DATA_PLAYER_DATA_VECTOR: usize = 0x48;
const PLAYER_DATA_INVENTORIES_VECTOR: usize = 0x320;
const INVENTORY_DESCRIPTOR_STRIDE: usize = 0x18;
const FLASK_INVENTORY_ID: i32 = 12;

const INVENTORY_TOTAL_BOXES: usize = 0x150;
const INVENTORY_ITEM_LIST: usize = 0x170;
const INVENTORY_ITEM_ENTITY: usize = 0x00;

const CHARGES_INTERNAL_PTR: usize = 0x10;
const CHARGES_CURRENT: usize = 0x18;
const CHARGES_INTERNAL_PER_USE: usize = 0x18;

const POE2_FLASK_SLOT_COUNT: usize = 2;
const MAX_PLAYER_DATA_POINTERS: usize = 16;
const MAX_INVENTORIES: usize = 256;
const MAX_INVENTORY_CELLS: usize = 64;
const MAX_REASONABLE_CHARGES: i32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlaskChargeState {
    pub inventory_item: usize,
    pub item_entity: usize,
    pub charges_component: usize,
    pub current: i32,
    pub per_use: i32,
}

impl FlaskChargeState {
    pub fn usable(self) -> bool {
        self.current >= self.per_use
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlaskChargeSnapshot {
    pub slots: [Option<FlaskChargeState>; POE2_FLASK_SLOT_COUNT],
}

#[derive(Debug, Clone, Copy)]
pub struct FlaskInventoryProbe {
    pub area_instance: usize,
    pub local_player: usize,
    pub player_info_offset: usize,
    pub server_data: usize,
    pub player_data: usize,
    pub flask_inventory: usize,
    pub width: i32,
    pub height: i32,
    pub item_cells: usize,
}

#[derive(Debug, Clone, Copy)]
struct CachedChargeComponent {
    inventory_item: usize,
    item_entity: usize,
    charges_component: usize,
}

/// Caches named Charges components while the corresponding inventory-item pointer is unchanged.
#[derive(Debug, Default)]
pub struct FlaskChargeCache {
    slots: [Option<CachedChargeComponent>; POE2_FLASK_SLOT_COUNT],
}

impl FlaskChargeCache {
    pub fn clear(&mut self) {
        self.slots = [None; POE2_FLASK_SLOT_COUNT];
    }
}

pub fn resolve_flask_inventory_probe(
    process: &Process,
    vitals: &VitalsProbe,
) -> io::Result<FlaskInventoryProbe> {
    let player_info_offset = vitals
        .local_player_offset
        .checked_sub(PLAYER_INFO_LOCAL_PLAYER)
        .ok_or_else(|| invalid("LocalPlayer offset is smaller than PlayerInfo.LocalPlayerPtr"))?;
    let player_info = vitals.area_instance + player_info_offset;

    let current_player = read_ptr(process, player_info + PLAYER_INFO_LOCAL_PLAYER)?;
    if current_player != vitals.local_player {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "PlayerInfo.LocalPlayerPtr no longer matches the resolved local player",
        ));
    }

    let server_data = read_ptr(process, player_info)?;
    let (player_data_first, player_data_count) = read_vector(
        process,
        server_data + SERVER_DATA_PLAYER_DATA_VECTOR,
        8,
        MAX_PLAYER_DATA_POINTERS,
        "ServerData.PlayerServerDataPtr",
    )?;
    if player_data_count == 0 {
        return Err(invalid("ServerData.PlayerServerDataPtr is empty"));
    }
    let player_data = read_ptr(process, player_data_first)?;

    let (inventories_first, inventories_count) = read_vector(
        process,
        player_data + PLAYER_DATA_INVENTORIES_VECTOR,
        INVENTORY_DESCRIPTOR_STRIDE,
        MAX_INVENTORIES,
        "playerData.PlayerInventories",
    )?;

    let mut flask_inventory = None;
    for index in 0..inventories_count {
        let entry = inventories_first + index * INVENTORY_DESCRIPTOR_STRIDE;
        let inventory_id = process.read_i32(entry)?;
        if inventory_id != FLASK_INVENTORY_ID {
            continue;
        }
        flask_inventory = Some(read_ptr(process, entry + 0x08)?);
        break;
    }

    let flask_inventory = flask_inventory.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "PoE2 flask inventory (id 12) was not found in ServerData",
        )
    })?;

    let width = process.read_i32(flask_inventory + INVENTORY_TOTAL_BOXES)?;
    let height = process.read_i32(flask_inventory + INVENTORY_TOTAL_BOXES + 4)?;
    if width <= 0 || height <= 0 {
        return Err(invalid(format!(
            "implausible PoE2 flask inventory dimensions: {width}x{height}"
        )));
    }
    let expected_cells = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| invalid("flask inventory cell-count overflow"))?;
    if expected_cells > MAX_INVENTORY_CELLS {
        return Err(invalid(format!(
            "implausible PoE2 flask inventory size: {width}x{height}"
        )));
    }

    let (_, item_cells) = read_vector(
        process,
        flask_inventory + INVENTORY_ITEM_LIST,
        8,
        MAX_INVENTORY_CELLS,
        "FlaskInventory.ItemList",
    )?;
    if item_cells != expected_cells {
        return Err(invalid(format!(
            "flask inventory dimensions imply {expected_cells} cells but ItemList contains {item_cells}"
        )));
    }
    if item_cells < POE2_FLASK_SLOT_COUNT {
        return Err(invalid(format!(
            "PoE2 flask inventory exposes only {item_cells} cells; expected at least two"
        )));
    }

    let probe = FlaskInventoryProbe {
        area_instance: vitals.area_instance,
        local_player: vitals.local_player,
        player_info_offset,
        server_data,
        player_data,
        flask_inventory,
        width,
        height,
        item_cells,
    };

    // Empty slots are valid; populated slots must expose a readable Charges component.
    let _ = sample_flask_charges(process, &probe)?;
    Ok(probe)
}

/// Full charge reader used by --debug.
pub fn sample_flask_charges(
    process: &Process,
    probe: &FlaskInventoryProbe,
) -> io::Result<FlaskChargeSnapshot> {
    let player_info = probe.area_instance + probe.player_info_offset;
    let current_player = read_ptr(process, player_info + PLAYER_INFO_LOCAL_PLAYER)?;
    if current_player != probe.local_player {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "local player changed while reading flask inventory",
        ));
    }
    let current_server_data = read_ptr(process, player_info)?;
    if current_server_data != probe.server_data {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "ServerData changed while reading flask inventory",
        ));
    }

    let (item_first, item_cells) = read_vector(
        process,
        probe.flask_inventory + INVENTORY_ITEM_LIST,
        8,
        MAX_INVENTORY_CELLS,
        "FlaskInventory.ItemList",
    )?;
    if item_cells != probe.item_cells {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "flask inventory shape changed",
        ));
    }

    let mut slots: [Option<FlaskChargeState>; POE2_FLASK_SLOT_COUNT] = [None; 2];
    for (slot, output) in slots.iter_mut().enumerate() {
        let inventory_item_raw = process.read_u64(item_first + slot * 8)? as usize;
        if inventory_item_raw == 0 {
            continue;
        }
        if !plausible_ptr(inventory_item_raw) {
            return Err(invalid(format!(
                "flask slot {} has an invalid inventory-item pointer",
                slot + 1
            )));
        }

        let item_entity = read_ptr(process, inventory_item_raw + INVENTORY_ITEM_ENTITY)?;
        let charges_component =
            resolve_component(process, item_entity, "Charges").ok_or_else(|| {
                invalid(format!(
                    "flask slot {} item does not expose a readable Charges component",
                    slot + 1
                ))
            })?;

        let (current, per_use) = read_charge_values(process, charges_component, slot)?;
        *output = Some(FlaskChargeState {
            inventory_item: inventory_item_raw,
            item_entity,
            charges_component,
            current,
            per_use,
        });
    }

    Ok(FlaskChargeSnapshot { slots })
}

/// Optimized charge reader used by auto mode.
pub fn sample_flask_charges_fast(
    process: &Process,
    probe: &FlaskInventoryProbe,
    cache: &mut FlaskChargeCache,
) -> io::Result<FlaskChargeSnapshot> {
    let player_info = probe.area_instance + probe.player_info_offset;

    // ServerData and LocalPlayer are 0x20 apart, so validate both with one process read.
    let player_info_bytes = process.read_bytes(player_info, PLAYER_INFO_LOCAL_PLAYER + 8)?;
    let current_server_data = read_u64_from(&player_info_bytes, 0) as usize;
    let current_player = read_u64_from(&player_info_bytes, PLAYER_INFO_LOCAL_PLAYER) as usize;
    if current_player != probe.local_player {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "local player changed while reading flask inventory",
        ));
    }
    if current_server_data != probe.server_data {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "ServerData changed while reading flask inventory",
        ));
    }

    // Read the vector header and the two flask-slot pointers in one call each.
    let header = process.read_bytes(probe.flask_inventory + INVENTORY_ITEM_LIST, 16)?;
    let item_first = read_u64_from(&header, 0) as usize;
    let item_last = read_u64_from(&header, 8) as usize;
    let item_cells = validate_vector_range(
        item_first,
        item_last,
        8,
        MAX_INVENTORY_CELLS,
        "FlaskInventory.ItemList",
    )?;
    if item_cells != probe.item_cells || item_cells < POE2_FLASK_SLOT_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "flask inventory shape changed",
        ));
    }

    let slot_pointers = process.read_bytes(item_first, POE2_FLASK_SLOT_COUNT * 8)?;
    let mut slots: [Option<FlaskChargeState>; POE2_FLASK_SLOT_COUNT] = [None; 2];

    for (slot, output) in slots.iter_mut().enumerate() {
        let inventory_item = read_u64_from(&slot_pointers, slot * 8) as usize;
        if inventory_item == 0 {
            cache.slots[slot] = None;
            continue;
        }
        if !plausible_ptr(inventory_item) {
            return Err(invalid(format!(
                "flask slot {} has an invalid inventory-item pointer",
                slot + 1
            )));
        }

        let cached = cache.slots[slot].filter(|cached| cached.inventory_item == inventory_item);
        let resolved = match cached {
            Some(cached) => cached,
            None => {
                let item_entity = read_ptr(process, inventory_item + INVENTORY_ITEM_ENTITY)?;
                let charges_component = resolve_component(process, item_entity, "Charges")
                    .ok_or_else(|| {
                        invalid(format!(
                            "flask slot {} item does not expose a readable Charges component",
                            slot + 1
                        ))
                    })?;
                let resolved = CachedChargeComponent {
                    inventory_item,
                    item_entity,
                    charges_component,
                };
                cache.slots[slot] = Some(resolved);
                resolved
            }
        };

        // Charge values stay live; only named-component discovery is cached.
        let values = read_charge_values(process, resolved.charges_component, slot);
        let (current, per_use) = match values {
            Ok(values) => values,
            Err(_) if cached.is_some() => {
                // Re-resolve once in case the component changed without the inventory pointer.
                cache.slots[slot] = None;
                let item_entity = read_ptr(process, inventory_item + INVENTORY_ITEM_ENTITY)?;
                let charges_component = resolve_component(process, item_entity, "Charges")
                    .ok_or_else(|| {
                        invalid(format!(
                            "flask slot {} item does not expose a readable Charges component",
                            slot + 1
                        ))
                    })?;
                let refreshed = CachedChargeComponent {
                    inventory_item,
                    item_entity,
                    charges_component,
                };
                cache.slots[slot] = Some(refreshed);
                read_charge_values(process, charges_component, slot)?
            }
            Err(error) => return Err(error),
        };

        let cache_entry = cache.slots[slot].expect("populated slot cache must exist");
        *output = Some(FlaskChargeState {
            inventory_item,
            item_entity: cache_entry.item_entity,
            charges_component: cache_entry.charges_component,
            current,
            per_use,
        });
    }

    Ok(FlaskChargeSnapshot { slots })
}

fn read_charge_values(
    process: &Process,
    charges_component: usize,
    slot: usize,
) -> io::Result<(i32, i32)> {
    // The internal pointer and current charge count fit in one 12-byte read.
    let bytes = process.read_bytes(charges_component + CHARGES_INTERNAL_PTR, 12)?;
    let charges_internal = read_u64_from(&bytes, 0) as usize;
    if !plausible_ptr(charges_internal) {
        return Err(invalid(format!(
            "flask slot {} has an invalid Charges internal pointer",
            slot + 1
        )));
    }
    let current = i32::from_le_bytes(
        bytes[CHARGES_CURRENT - CHARGES_INTERNAL_PTR..CHARGES_CURRENT - CHARGES_INTERNAL_PTR + 4]
            .try_into()
            .expect("fixed current-charge field"),
    );
    let per_use = process.read_i32(charges_internal + CHARGES_INTERNAL_PER_USE)?;

    if !(0..=MAX_REASONABLE_CHARGES).contains(&current)
        || !(1..=MAX_REASONABLE_CHARGES).contains(&per_use)
    {
        return Err(invalid(format!(
            "flask slot {} returned implausible charge values: current={current}, per_use={per_use}",
            slot + 1
        )));
    }
    Ok((current, per_use))
}

fn read_vector(
    process: &Process,
    address: usize,
    stride: usize,
    max_count: usize,
    label: &str,
) -> io::Result<(usize, usize)> {
    let first = process.read_u64(address)? as usize;
    let last = process.read_u64(address + 8)? as usize;
    let count = validate_vector_range(first, last, stride, max_count, label)?;
    Ok((first, count))
}

fn validate_vector_range(
    first: usize,
    last: usize,
    stride: usize,
    max_count: usize,
    label: &str,
) -> io::Result<usize> {
    if first == 0 && last == 0 {
        return Ok(0);
    }
    if !plausible_ptr(first) || last < first {
        return Err(invalid(format!("{label} has an invalid range")));
    }
    let bytes = last - first;
    if stride == 0 || bytes % stride != 0 {
        return Err(invalid(format!("{label} has an invalid element stride")));
    }
    let count = bytes / stride;
    if count > max_count {
        return Err(invalid(format!(
            "{label} contains an implausible {count} elements"
        )));
    }
    Ok(count)
}

fn read_ptr(process: &Process, address: usize) -> io::Result<usize> {
    let value = process.read_u64(address)? as usize;
    if plausible_ptr(value) {
        Ok(value)
    } else {
        Err(invalid(format!("invalid pointer at 0x{address:016X}")))
    }
}

fn read_u64_from(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated 8-byte field"),
    )
}

fn plausible_ptr(value: usize) -> bool {
    value >= 0x1_0000
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
