mod args;
mod inventory;
mod network;
mod level;

use args::{ClientArgs, GameMode};
use bevy::{asset::AssetPlugin, input::mouse::{MouseMotion, MouseWheel}, prelude::*};
use clap::Parser;
use inventory::*;
use network::*;
use level::{LoadedLevel, LevelCollider, LevelRuntimePlugin};
use std::time::Duration;

#[derive(Resource, Clone)]
pub(crate) struct RuntimeConfig(pub(crate) ClientArgs);

#[derive(Component)]
pub(crate) struct Player;

#[derive(Component)]
pub(crate) struct Pickup(pub(crate) ItemStack);

#[derive(Component)]
struct PickupPrompt;

#[derive(Component)]
struct StatsText;

#[derive(Resource)]
pub(crate) struct PlayerStats {
    pub(crate) hunger: f32,
    pub(crate) sanity: f32,
    drain: Timer,
}

#[derive(Resource, Default)]
struct LookAngles { yaw: f32, pitch: f32 }

#[derive(Resource)]
struct NetSendTimer(Timer);

fn main() {
    let args = ClientArgs::parse();
    if args.mode == GameMode::Multiplayer && args.nickname.trim().is_empty() {
        eprintln!("nickname cannot be empty in multiplayer");
        std::process::exit(2);
    }

    let loaded_level = match level::prepare_level(&args.level, args.data_dir.as_deref()) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("could not load level: {err:#}");
            std::process::exit(3);
        }
    };
    let asset_root = loaded_level.root.to_string_lossy().into_owned();
    println!(
        "loading level {} ({}) format v{} by {} from {}",
        loaded_level.definition.meta.name,
        loaded_level.definition.meta.id,
        loaded_level.definition.version,
        loaded_level.definition.meta.author,
        loaded_level.root.display()
    );

    let mut app = App::new();
    app.insert_resource(RuntimeConfig(args.clone()))
        .insert_resource(loaded_level)
        .insert_resource(Inventory::default())
        .insert_resource(PlayerStats { hunger: 100.0, sanity: 100.0, drain: Timer::new(Duration::from_secs(2), TimerMode::Repeating) })
        .insert_resource(LookAngles::default())
        .insert_resource(NetSendTimer(Timer::new(Duration::from_millis(100), TimerMode::Repeating)))
        .add_plugins(DefaultPlugins
            .set(AssetPlugin { file_path: asset_root, ..default() })
            .set(WindowPlugin {
                primary_window: Some(Window { title: "Backrooms Infinity".into(), resolution: (1280., 720.).into(), ..default() }),
                ..default()
            }))
        .add_plugins(LevelRuntimePlugin)
        .add_systems(Startup, (setup_player, setup_ui))
        .add_systems(Update, (
            player_movement, mouse_look, cycle_hotbar, toggle_inventory,
            update_inventory_visibility, update_slot_labels, inventory_interaction,
            pickup_system, update_pickup_prompt, drain_stats, consume_selected,
            update_hud, craft_system, sync_multiplayer,
        ));

    if args.mode == GameMode::Multiplayer {
        app.insert_resource(spawn_bridge(args.bridge_ws));
    }

    app.run();
}

fn setup_player(mut commands: Commands, level: Res<LoadedLevel>, mut angles: ResMut<LookAngles>) {
    let spawn = Vec3::from_array(level.definition.player.spawn);
    angles.yaw = level.definition.player.yaw_deg.to_radians();
    angles.pitch = level.definition.player.pitch_deg.to_radians();
    commands.spawn((
        Player,
        Camera3d::default(),
        Transform::from_translation(spawn).with_rotation(Quat::from_euler(EulerRot::YXZ, angles.yaw, angles.pitch, 0.0)),
    ));
}

pub(crate) fn spawn_pickup(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, item: ItemStack, pos: Vec3) -> Entity {
    commands.spawn((
        Pickup(item),
        Mesh3d(meshes.add(Cuboid::new(0.35, 0.35, 0.35))),
        MeshMaterial3d(materials.add(Color::srgb(0.75, 0.72, 0.30))),
        Transform::from_translation(pos),
    )).id()
}

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        StatsText,
        Text::new("Hunger 100 | Sanity 100"),
        TextFont { font_size: 20.0, ..default() },
        Node { position_type: PositionType::Absolute, left: Val::Px(16.0), top: Val::Px(12.0), ..default() },
    ));
    commands.spawn((
        PickupPrompt,
        Text::new(""),
        TextFont { font_size: 22.0, ..default() },
        Node { position_type: PositionType::Absolute, left: Val::Percent(38.0), bottom: Val::Px(120.0), ..default() },
    ));
    commands.spawn((
        HotbarHud,
        Text::new("[1] Empty"),
        TextFont { font_size: 24.0, ..default() },
        Node { position_type: PositionType::Absolute, right: Val::Px(24.0), bottom: Val::Px(24.0), padding: UiRect::all(Val::Px(10.0)), ..default() },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.75)),
    ));

    commands.spawn((
        InventoryRoot,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: Val::Percent(15.0), top: Val::Percent(10.0),
            width: Val::Percent(70.0), height: Val::Percent(80.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0), padding: UiRect::all(Val::Px(20.0)), ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.04, 0.04, 0.94)),
    )).with_children(|root| {
        root.spawn((Text::new("BACKPACK — drag with LMB | E closes"), TextFont { font_size: 24.0, ..default() }));
        root.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), row_gap: Val::Px(8.0), ..default() }).with_children(|row| {
            for index in 0..BACKPACK_SLOTS { spawn_slot(row, SlotKind::Backpack, index); }
        });
        root.spawn((Text::new("HOTBAR"), TextFont { font_size: 20.0, ..default() }));
        root.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
            for index in 0..HOTBAR_SLOTS { spawn_slot(row, SlotKind::Hotbar, index); }
        });
        root.spawn((Text::new("CRAFTING (4 slots)"), TextFont { font_size: 20.0, ..default() }));
        root.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
            for index in 0..CRAFT_SLOTS { spawn_slot(row, SlotKind::Craft, index); }
        });
        root.spawn((
            CraftButton,
            Button,
            Node { width: Val::Px(180.0), height: Val::Px(42.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgb(0.25, 0.25, 0.12)),
        )).with_child((Text::new("Craft"), TextFont { font_size: 20.0, ..default() }));
    });
}

fn spawn_slot(parent: &mut ChildBuilder, kind: SlotKind, index: usize) {
    parent.spawn((
        InventorySlot { kind, index },
        Button,
        Node { width: Val::Px(120.0), height: Val::Px(54.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(Color::srgb(0.13, 0.13, 0.13)),
    )).with_child((SlotLabel, Text::new("Empty"), TextFont { font_size: 14.0, ..default() }));
}

fn player_movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut Transform, With<Player>>,
    colliders: Query<(&Transform, &LevelCollider), Without<Player>>,
    inventory: Res<Inventory>,
    level: Res<LoadedLevel>,
) {
    if inventory.open { return; }
    let Ok(mut t) = q.get_single_mut() else { return; };
    let mut input = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) { input.z -= 1.0; }
    if keys.pressed(KeyCode::KeyS) { input.z += 1.0; }
    if keys.pressed(KeyCode::KeyA) { input.x -= 1.0; }
    if keys.pressed(KeyCode::KeyD) { input.x += 1.0; }
    if input.length_squared() == 0.0 { return; }

    let forward = t.forward().as_vec3();
    let right = t.right().as_vec3();
    let planar = (forward * -input.z + right * input.x).with_y(0.0).normalize_or_zero();
    let delta = planar * level.definition.player.move_speed * time.delta_secs();

    // Simple capsule-like collision against axis-aligned level colliders. Movement is
    // resolved per axis so the player slides along walls instead of sticking to corners.
    let mut candidate = t.translation;
    candidate.x += delta.x;
    if !player_hits_level(candidate, &colliders) { t.translation.x = candidate.x; }
    candidate = t.translation;
    candidate.z += delta.z;
    if !player_hits_level(candidate, &colliders) { t.translation.z = candidate.z; }
}

fn player_hits_level(position: Vec3, colliders: &Query<(&Transform, &LevelCollider), Without<Player>>) -> bool {
    const PLAYER_RADIUS: f32 = 0.30;
    const PLAYER_HALF_HEIGHT: f32 = 0.85;
    colliders.iter().any(|(transform, collider)| {
        let d = position - transform.translation;
        d.x.abs() < collider.half_extents.x + PLAYER_RADIUS
            && d.z.abs() < collider.half_extents.z + PLAYER_RADIUS
            && d.y.abs() < collider.half_extents.y + PLAYER_HALF_HEIGHT
    })
}

fn mouse_look(mut motion: EventReader<MouseMotion>, mut angles: ResMut<LookAngles>, mut q: Query<&mut Transform, With<Player>>, inventory: Res<Inventory>) {
    if inventory.open { motion.clear(); return; }
    let delta = motion.read().fold(Vec2::ZERO, |acc, ev| acc + ev.delta);
    if delta == Vec2::ZERO { return; }
    angles.yaw -= delta.x * 0.0025;
    angles.pitch = (angles.pitch - delta.y * 0.0025).clamp(-1.45, 1.45);
    if let Ok(mut t) = q.get_single_mut() { t.rotation = Quat::from_euler(EulerRot::YXZ, angles.yaw, angles.pitch, 0.0); }
}

fn cycle_hotbar(mut wheel: EventReader<MouseWheel>, mut inv: ResMut<Inventory>) {
    let dy: f32 = wheel.read().map(|e| e.y).sum();
    if dy > 0.0 { inv.selected = (inv.selected + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS; }
    if dy < 0.0 { inv.selected = (inv.selected + 1) % HOTBAR_SLOTS; }
}

fn toggle_inventory(keys: Res<ButtonInput<KeyCode>>, mut inv: ResMut<Inventory>) {
    if keys.just_pressed(KeyCode::KeyE) { inv.open = !inv.open; inv.dragging = None; }
}

fn update_inventory_visibility(inv: Res<Inventory>, mut root: Query<&mut Node, With<InventoryRoot>>) {
    if !inv.is_changed() { return; }
    if let Ok(mut node) = root.get_single_mut() { node.display = if inv.open { Display::Flex } else { Display::None }; }
}

fn update_slot_labels(inv: Res<Inventory>, slots: Query<(&InventorySlot, &Children)>, mut labels: Query<&mut Text, With<SlotLabel>>) {
    if !inv.is_changed() { return; }
    for (slot, children) in &slots {
        let value = inv.slots(slot.kind).get(slot.index).and_then(|x| x.as_ref());
        for child in children.iter() {
            if let Ok(mut text) = labels.get_mut(child) {
                **text = match value { Some(s) => format!("{} x{}", display_name(&s.id), s.count), None => "Empty".into() };
            }
        }
    }
}

fn inventory_interaction(
    mouse: Res<ButtonInput<MouseButton>>,
    mut inv: ResMut<Inventory>,
    slots: Query<(&InventorySlot, &Interaction)>,
    player: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !inv.open { return; }
    if mouse.just_pressed(MouseButton::Left) {
        if let Some((slot, _)) = slots.iter().find(|(_, i)| **i == Interaction::Pressed) {
            if let Some(item) = inv.slots_mut(slot.kind).get_mut(slot.index).and_then(Option::take) {
                inv.dragging = Some((slot.kind, slot.index, item));
            }
        }
    }
    if mouse.just_released(MouseButton::Left) {
        let Some((from_kind, from_index, item)) = inv.dragging.take() else { return; };
        if let Some((target, _)) = slots.iter().find(|(_, i)| **i == Interaction::Hovered || **i == Interaction::Pressed) {
            let displaced = inv.slots_mut(target.kind).get_mut(target.index).and_then(|slot| slot.replace(item));
            if let Some(old) = displaced {
                if let Ok(t) = player.get_single() { spawn_pickup(&mut commands, &mut meshes, &mut materials, old, t.translation + t.forward().as_vec3()); }
            }
        } else if let Some(slot) = inv.slots_mut(from_kind).get_mut(from_index) {
            *slot = Some(item);
        }
    }
}

fn nearest_pickup(player: &Transform, pickups: &Query<(Entity, &Transform, &Pickup)>, max_distance: f32) -> Option<(Entity, String)> {
    let forward = player.forward().as_vec3();
    pickups.iter().filter_map(|(e, t, p)| {
        let delta = t.translation - player.translation;
        let dist = delta.length();
        if dist <= max_distance && delta.normalize_or_zero().dot(forward) > 0.55 { Some((dist, e, p.0.id.clone())) } else { None }
    }).min_by(|a,b| a.0.total_cmp(&b.0)).map(|(_,e,id)|(e,id))
}

fn update_pickup_prompt(player: Query<&Transform, With<Player>>, pickups: Query<(Entity, &Transform, &Pickup)>, mut prompt: Query<&mut Text, With<PickupPrompt>>, inv: Res<Inventory>, level: Res<LoadedLevel>) {
    let Ok(mut text) = prompt.get_single_mut() else { return; };
    if inv.open { **text = "".into(); return; }
    let Ok(p) = player.get_single() else { return; };
    **text = nearest_pickup(p, &pickups, level.definition.player.interaction_distance).map(|(_,id)| format!("F — take {}", display_name(&id))).unwrap_or_default();
}

fn pickup_system(keys: Res<ButtonInput<KeyCode>>, player: Query<&Transform, With<Player>>, pickups: Query<(Entity, &Transform, &Pickup)>, mut commands: Commands, mut inv: ResMut<Inventory>, level: Res<LoadedLevel>) {
    if !keys.just_pressed(KeyCode::KeyF) || inv.open { return; }
    let Ok(p) = player.get_single() else { return; };
    let Some((entity, _)) = nearest_pickup(p, &pickups, level.definition.player.interaction_distance) else { return; };
    if let Ok((_, _, pickup)) = pickups.get(entity) {
        if inv.insert_backpack(pickup.0.clone()).is_ok() { commands.entity(entity).despawn(); }
    }
}

fn drain_stats(time: Res<Time>, mut stats: ResMut<PlayerStats>) {
    stats.drain.tick(time.delta());
    if stats.drain.just_finished() {
        stats.hunger = (stats.hunger - 0.4).max(0.0);
        stats.sanity = (stats.sanity - 0.2).max(0.0);
    }
}

fn consume_selected(mouse: Res<ButtonInput<MouseButton>>, mut inv: ResMut<Inventory>, mut stats: ResMut<PlayerStats>) {
    if !mouse.just_pressed(MouseButton::Right) || inv.open { return; }
    let selected = inv.selected;
    let Some(stack) = inv.hotbar.get_mut(selected).and_then(Option::as_mut) else { return; };
    let consumed = match stack.id.as_str() {
        "almond_water" => { stats.hunger = (stats.hunger + 18.0).min(100.0); stats.sanity = (stats.sanity + 30.0).min(100.0); true }
        "rice_cookie" => { stats.hunger = (stats.hunger + 24.0).min(100.0); stats.sanity = (stats.sanity + 8.0).min(100.0); true }
        _ => false,
    };
    if consumed {
        stack.count -= 1;
        if stack.count == 0 { inv.hotbar[selected] = None; }
    }
}

fn update_hud(stats: Res<PlayerStats>, inv: Res<Inventory>, mut stats_text: Query<&mut Text, (With<StatsText>, Without<HotbarHud>)>, mut hotbar: Query<&mut Text, (With<HotbarHud>, Without<StatsText>)>) {
    if let Ok(mut t) = stats_text.get_single_mut() { **t = format!("Hunger {:03.0} | Sanity {:03.0}", stats.hunger, stats.sanity); }
    if let Ok(mut t) = hotbar.get_single_mut() {
        **t = match inv.hotbar.get(inv.selected).and_then(|x| x.as_ref()) {
            Some(s) => format!("[{}] {} x{}", inv.selected + 1, display_name(&s.id), s.count),
            None => format!("[{}] Empty", inv.selected + 1),
        };
    }
}

fn craft_system(
    buttons: Query<&Interaction, (With<CraftButton>, Changed<Interaction>)>,
    mut inv: ResMut<Inventory>,
) {
    if !buttons.iter().any(|i| *i == Interaction::Pressed) { return; }
    let counts = inv.crafting_counts();
    let Some(recipe) = recipes().into_iter().find(|r| r.inputs.iter().all(|(id,n)| counts.get(id).copied().unwrap_or(0) >= *n)) else { return; };
    inv.consume_recipe(&recipe);
    let _ = inv.insert_backpack(recipe.output);
}

fn sync_multiplayer(
    time: Res<Time>,
    config: Res<RuntimeConfig>,
    bridge: Option<Res<NetworkBridge>>,
    mut timer: ResMut<NetSendTimer>,
    player: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut remotes: Query<(Entity, &RemotePlayer, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if config.0.mode != GameMode::Multiplayer { return; }
    let Some(bridge) = bridge else { return; };
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        if let Ok(t) = player.get_single() {
            let (_, yaw, _) = t.rotation.to_euler(EulerRot::XYZ);
            let _ = bridge.outgoing.send(NetState { kind: "state".into(), nickname: config.0.nickname.clone(), x: t.translation.x, y: t.translation.y, z: t.translation.z, yaw });
        }
    }
    while let Ok(state) = bridge.incoming.try_recv() {
        if state.nickname == config.0.nickname || state.kind != "state" { continue; }
        if let Some((_entity, _, mut t)) = remotes.iter_mut().find(|(_, r, _)| r.nickname == state.nickname) {
            t.translation = Vec3::new(state.x, state.y, state.z);
            t.rotation = Quat::from_rotation_y(state.yaw);
        } else {
            commands.spawn((
                RemotePlayer { nickname: state.nickname },
                Mesh3d(meshes.add(Capsule3d::new(0.3, 1.2))),
                MeshMaterial3d(materials.add(Color::srgb(0.35, 0.55, 0.75))),
                Transform::from_xyz(state.x, state.y, state.z),
            ));
        }
    }
}
