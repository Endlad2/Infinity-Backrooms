//! Меню паузы (ESC). Английские подписи.

use bevy::prelude::*;

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum PauseState {
    Playing,
    Menu,
    Settings,
    Notes,
    Exporting,
}
impl Default for PauseState { fn default() -> Self { PauseState::Playing } }
impl PauseState {
    pub fn is_paused(&self) -> bool { !matches!(self, PauseState::Playing) }
}

#[derive(Component)]
pub struct PauseOverlay;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseButton {
    Resume, Settings, Export, Notes, Exit,
}

pub fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<PauseState>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        *state = match *state {
            PauseState::Playing => PauseState::Menu,
            _ => PauseState::Playing,
        };
    }
}

pub fn sync_pause_overlay(
    state: Res<PauseState>,
    mut q: Query<&mut Visibility, With<PauseOverlay>>,
) {
    let visible = state.is_paused();
    for mut v in q.iter_mut() {
        *v = if visible { Visibility::Visible } else { Visibility::Hidden };
    }
}

pub fn spawn_pause_overlay(mut commands: Commands) {
    commands
        .spawn((
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
                visibility: Visibility::Hidden,
                z_index: ZIndex::Global(100),
                ..default()
            },
            PauseOverlay,
        ))
        .with_children(|parent| {
            parent
                .spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(12.0),
                        padding: UiRect::all(Val::Px(24.0)),
                        ..default()
                    },
                    ..default()
                })
                .with_children(|col| {
                    col.spawn(TextBundle::from_section(
                        "PAUSED",
                        TextStyle { font_size: 40.0, color: Color::WHITE, ..default() },
                    ));
                    for (btn, label) in [
                        (PauseButton::Resume, "Resume"),
                        (PauseButton::Settings, "Settings"),
                        (PauseButton::Export, "Export Level"),
                        (PauseButton::Notes, "Add AI Note"),
                        (PauseButton::Exit, "Exit"),
                    ] {
                        col.spawn((
                            ButtonBundle {
                                style: Style {
                                    padding: UiRect::axes(Val::Px(24.0), Val::Px(10.0)),
                                    min_width: Val::Px(260.0),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                background_color: BackgroundColor(Color::srgb(0.15, 0.15, 0.2)),
                                ..default()
                            },
                            btn,
                        ))
                        .with_children(|b| {
                            b.spawn(TextBundle::from_section(
                                label,
                                TextStyle { font_size: 22.0, color: Color::WHITE, ..default() },
                            ));
                        });
                    }
                });
        });
}

#[derive(Event, Debug)]
pub struct ExportLevelRequest;

pub fn handle_pause_buttons(
    mut state: ResMut<PauseState>,
    mut exit: EventWriter<AppExit>,
    mut export_req: EventWriter<ExportLevelRequest>,
    interactions: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
) {
    for (interaction, btn) in interactions.iter() {
        if *interaction != Interaction::Pressed { continue; }
        match btn {
            PauseButton::Resume => *state = PauseState::Playing,
            PauseButton::Settings => *state = PauseState::Settings,
            PauseButton::Notes => *state = PauseState::Notes,
            PauseButton::Export => {
                export_req.send(ExportLevelRequest);
                *state = PauseState::Exporting;
            }
            PauseButton::Exit => { exit.send(AppExit::Success); }
        }
    }
}

#[derive(Default)]
pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseState>()
            .add_event::<ExportLevelRequest>()
            .add_systems(Startup, spawn_pause_overlay)
            .add_systems(Update, (toggle_pause, sync_pause_overlay, handle_pause_buttons));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_playing() {
        assert_eq!(PauseState::default(), PauseState::Playing);
    }
}
