//! Экран загрузки (§6.2 ТЗ): чёрный фон + «Уровень генерируется...» внизу по центру.
//! Показывается во время генерации уровня ИИ.

use bevy::prelude::*;

/// Состояние загрузочного экрана.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Loading,
    Done,
}

impl Default for LoadingState {
    fn default() -> Self {
        LoadingState::Idle
    }
}

/// Маркер корневого узла экрана загрузки.
#[derive(Component)]
pub struct LoadingRoot;

/// Маркер текста статуса.
#[derive(Component)]
pub struct LoadingText;

/// Система: показывать/скрывать оверлей по состоянию.
pub fn sync_loading_overlay(
    state: Res<LoadingState>,
    mut q: Query<&mut Visibility, With<LoadingRoot>>,
) {
    let visible = matches!(*state, LoadingState::Loading);
    for mut v in q.iter_mut() {
        *v = if visible { Visibility::Visible } else { Visibility::Hidden };
    }
}

/// Система: анимированные точки после «Уровень генерируется».
pub fn animate_loading_text(
    time: Res<Time>,
    state: Res<LoadingState>,
    mut acc: Local<f32>,
    mut phase: Local<usize>,
    mut q: Query<&mut Text, With<LoadingText>>,
) {
    if !matches!(*state, LoadingState::Loading) {
        return;
    }
    *acc += time.delta_seconds();
    if *acc > 0.5 {
        *acc = 0.0;
        *phase = (*phase + 1) % 4;
    }
    let dots = match *phase {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    };
    for mut txt in q.iter_mut() {
        if let Some(section) = txt.sections.get_mut(0) {
            section.value = format!("Уровень генерируется{dots}");
        }
    }
}

/// Создаёт экран загрузки.
pub fn spawn_loading_overlay(mut commands: Commands) {
    commands
        .spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    padding: UiRect::bottom(Val::Px(60.0)),
                    ..default()
                },
                background_color: BackgroundColor(Color::BLACK),
                visibility: Visibility::Hidden,
                z_index: ZIndex::Global(50),
                ..default()
            },
            LoadingRoot,
        ))
        .with_children(|c| {
            c.spawn((
                TextBundle::from_section(
                    "Уровень генерируется...",
                    TextStyle {
                        font_size: 28.0,
                        color: Color::WHITE,
                        ..default()
                    },
                ),
                LoadingText,
            ));
        });
}

/// Плагин экрана загрузки.
#[derive(Default)]
pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingState>()
            .add_systems(Startup, spawn_loading_overlay)
            .add_systems(Update, (sync_loading_overlay, animate_loading_text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_defaults_to_idle() {
        assert_eq!(LoadingState::default(), LoadingState::Idle);
    }

    #[test]
    fn plugin_builds() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.add_plugins(LoadingPlugin);
        app.update();
    }
}