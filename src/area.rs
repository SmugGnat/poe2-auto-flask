//! Shared, validated area information for both games.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaInfo {
    pub id: String,
    pub name: String,
    pub act: i32,
    pub is_town: bool,
    pub is_hideout: bool,
    pub world_data_offset: usize,
    pub world_data: usize,
    pub area_row: usize,
}

impl AreaInfo {
    pub fn allows_auto_flask(&self, enable_in_hideout: bool) -> bool {
        !self.is_town && (!self.is_hideout || enable_in_hideout)
    }

    pub fn kind(&self) -> &'static str {
        if self.is_town {
            "town"
        } else if self.is_hideout {
            "hideout"
        } else {
            "combat area"
        }
    }
}
