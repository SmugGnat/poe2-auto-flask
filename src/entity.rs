use crate::process::Process;

const ENTITY_DETAILS_PTR: usize = 0x08;
const ENTITY_COMPONENT_LIST: usize = 0x10;
const ENTITY_DETAILS_COMPONENT_LOOKUP_PTR: usize = 0x28;
const COMPONENT_LOOKUP_BUCKET: usize = 0x28;
const COMPONENT_LOOKUP_ENTRY_STRIDE: usize = 0x10;
const MAX_COMPONENTS: usize = 256;

/// Resolves a named component from an entity's component table.
pub fn resolve_component(process: &Process, entity: usize, wanted: &str) -> Option<usize> {
    let details = read_ptr(process, entity + ENTITY_DETAILS_PTR)?;
    let lookup = read_ptr(process, details + ENTITY_DETAILS_COMPONENT_LOOKUP_PTR)?;

    let component_first = read_ptr(process, entity + ENTITY_COMPONENT_LIST)?;
    let component_last = read_ptr(process, entity + ENTITY_COMPONENT_LIST + 8)?;
    let component_count = checked_count(component_first, component_last, 8, MAX_COMPONENTS)?;

    let bucket_first = read_ptr(process, lookup + COMPONENT_LOOKUP_BUCKET)?;
    let bucket_last = read_ptr(process, lookup + COMPONENT_LOOKUP_BUCKET + 8)?;
    let entry_count = checked_count(
        bucket_first,
        bucket_last,
        COMPONENT_LOOKUP_ENTRY_STRIDE,
        MAX_COMPONENTS,
    )?;

    for index in 0..entry_count {
        let entry = bucket_first + index * COMPONENT_LOOKUP_ENTRY_STRIDE;
        let Some(name_ptr) = read_ptr(process, entry) else {
            continue;
        };
        let Ok(component_index) = process.read_i32(entry + 8) else {
            continue;
        };
        if component_index < 0 || component_index as usize >= component_count {
            continue;
        }

        let Ok(name) = process.read_utf8_z(name_ptr, 32) else {
            continue;
        };
        if name == wanted {
            return read_ptr(process, component_first + component_index as usize * 8);
        }
    }

    None
}

fn checked_count(first: usize, last: usize, stride: usize, max_count: usize) -> Option<usize> {
    if last <= first {
        return None;
    }
    let bytes = last - first;
    if bytes % stride != 0 {
        return None;
    }
    let count = bytes / stride;
    (count != 0 && count <= max_count).then_some(count)
}

fn read_ptr(process: &Process, address: usize) -> Option<usize> {
    let value = process.read_u64(address).ok()? as usize;
    (value >= 0x1_0000).then_some(value)
}
