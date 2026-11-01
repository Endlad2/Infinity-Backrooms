use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const BACKPACK_SLOTS: usize = 10;
pub const HOTBAR_SLOTS: usize = 8;
pub const CRAFT_SLOTS: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStack {
    pub id: String,
    pub count: u16,
}

impl ItemStack {
    pub fn one(id: impl Into<String>) -> Self {
        Self { id: id.into(), count: 1 }
    }
}

#[derive(Resource)]
pub struct Inventory {
    pub backpack: Vec<Option<ItemStack>>,
    pub hotbar: Vec<Option<ItemStack>>,
    pub crafting: Vec<Option<ItemStack>>,
    pub selected: usize,
    pub open: bool,
    pub dragging: Option<(SlotKind, usize, ItemStack)>,
}

impl Default for Inventory {
    fn default() -> Self {
        let mut hotbar = vec![None; HOTBAR_SLOTS];
        hotbar[0] = Some(ItemStack::one("almond_water"));
        hotbar[1] = Some(ItemStack::one("rice_cookie"));
        let mut backpack = vec![None; BACKPACK_SLOTS];
        backpack[0] = Some(ItemStack::one("wood"));
        backpack[1] = Some(ItemStack::one("wood"));
        Self {
            backpack,
            hotbar,
            crafting: vec![None; CRAFT_SLOTS],
            selected: 0,
            open: false,
            dragging: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub enum SlotKind {
    Backpack,
    Hotbar,
    Craft,
}

#[derive(Component)]
pub struct InventorySlot {
    pub kind: SlotKind,
    pub index: usize,
}

#[derive(Component)]
pub struct InventoryRoot;

#[derive(Component)]
pub struct HotbarHud;

#[derive(Component)]
pub struct SlotLabel;

#[derive(Component)]
pub struct CraftButton;

#[derive(Debug, Clone)]
pub struct Recipe {
    pub inputs: Vec<(&'static str, u16)>,
    pub output: ItemStack,
}

pub fn recipes() -> Vec<Recipe> {
    vec![
        Recipe { inputs: vec![("wood", 2)], output: ItemStack::one("plank_shiv") },
        Recipe { inputs: vec![("wood", 4)], output: ItemStack::one("workbench") },
        Recipe { inputs: vec![("wood", 2), ("scrap", 2)], output: ItemStack::one("purifier") },
    ]
}

pub fn display_name(id: &str) -> &str {
    match id {
        "almond_water" => "Almond Water",
        "rice_cookie" => "Rice Cookie",
        "wood" => "Wood",
        "scrap" => "Scrap",
        "plank_shiv" => "Plank Shiv",
        "workbench" => "Workbench",
        "purifier" => "Purifier",
        other => other,
    }
}

impl Inventory {
    pub fn slots(&self, kind: SlotKind) -> &Vec<Option<ItemStack>> {
        match kind {
            SlotKind::Backpack => &self.backpack,
            SlotKind::Hotbar => &self.hotbar,
            SlotKind::Craft => &self.crafting,
        }
    }

    pub fn slots_mut(&mut self, kind: SlotKind) -> &mut Vec<Option<ItemStack>> {
        match kind {
            SlotKind::Backpack => &mut self.backpack,
            SlotKind::Hotbar => &mut self.hotbar,
            SlotKind::Craft => &mut self.crafting,
        }
    }

    pub fn insert_backpack(&mut self, stack: ItemStack) -> Result<(), ItemStack> {
        if let Some(slot) = self.backpack.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(stack);
            Ok(())
        } else {
            Err(stack)
        }
    }

    pub fn crafting_counts(&self) -> HashMap<&str, u16> {
        let mut counts = HashMap::new();
        for stack in self.crafting.iter().flatten() {
            *counts.entry(stack.id.as_str()).or_insert(0) += stack.count;
        }
        counts
    }

    pub fn consume_recipe(&mut self, recipe: &Recipe) {
        for (id, mut needed) in recipe.inputs.iter().copied() {
            for slot in &mut self.crafting {
                if needed == 0 { break; }
                let Some(stack) = slot.as_mut() else { continue; };
                if stack.id != id { continue; }
                let take = needed.min(stack.count);
                stack.count -= take;
                needed -= take;
                if stack.count == 0 { *slot = None; }
            }
        }
    }
}
