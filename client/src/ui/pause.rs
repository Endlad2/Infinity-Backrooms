//! Меню паузы (ESC): полупрозрачный затемняющий оверлей + кнопки.
//! Согласно §7.2 ТЗ: Продолжить / Настройки / Выйти / Добавить замечание ИИ.

use bevy::prelude::*;

/// Состояние паузы.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum PauseState {
    Playing,
    Menu,
    Settings,
    Notes,
}

impl Default for PauseState {
    fn default() -> Self {
        PauseState::Playing
    }
}

impl PauseState {
    pub fn is_paused(&self) -> bool {
        !matches!(self, PauseState::Playing)
    }
}

/// Маркер: корневой узел оверлея паузы.
#[derive(Component)]
pub struct PauseOverlay;

/// Компонент кнопки с действием.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseButton {
    Resume,
    Settings,
    Exit,
    Notes,
}

/// Система: ESC переключает паузу.
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

/// Система: показывать/скрывать оверлей по состоянию.
pub fn sync_pause_overlay(
    state: Res<PauseState>,
    mut q: Query<&mut Visibility, With<PauseOverlay>>,
) {
    let visible = state.is_paused();
    for mut v in q.iter_mut() {
        *v = if visible { Visibility::Visible } else { Visibility::Hidden };
    }
}

/// Заглушка UI-сборки оверлея. В полноценной версии — spawn NodeBundle с чёрным
/// полупрозрачным фоном и вертикальным списком кнопок.
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
                    for (btn, label) in [
                        (PauseButton::Resume, "Продолжить"),
                        (PauseButton::Settings, "Настройки"),
                        (PauseButton::Notes, "Добавить замечание ИИ"),
                        (PauseButton::Exit, "Выйти"),
                    ] {
                        col.spawn((
                            ButtonBundle {
                                style: Style {
                                    padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
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
                                TextStyle {
                                    font_size: 24.0,
                                    color: Color::WHITE,
                                    ..default()
                                },
                            ));
                        });
                    }
                });
        });
}

/// Обработка кликов по кнопкам.
pub fn handle_pause_buttons(
    mut state: ResMut<PauseState>,
    mut exit: EventWriter<AppExit>,
    interactions: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
) {
    for (interaction, btn) in interactions.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match btn {
            PauseButton::Resume => *state = PauseState::Playing,
            PauseButton::Settings => *state = PauseState::Settings,
            PauseButton::Notes => *state = PauseState::Notes,
            PauseButton::Exit => {
                exit.send(AppExit::Success);
            }
        }
    }
}

/// Плагин меню паузы.
#[derive(Default)]
pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseState>()
            .add_systems(Startup, spawn_pause_overlay)
            .add_systems(Update, (toggle_pause, sync_pause_overlay, handle_pause_buttons));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_state_default_is_playing() {
        assert_eq!(PauseState::default(), PauseState::Playing);
        assert!(!PauseState::Playing.is_paused());
        assert!(PauseState::Menu.is_paused());
    }

    #[test]
    fn plugin_builds_with_minimal() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.add_plugins(PausePlugin);
        app.update();
    }
}