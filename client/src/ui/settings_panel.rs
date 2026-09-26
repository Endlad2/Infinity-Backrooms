//! Панель настроек (§4.2 ТЗ): слайдер чувствительности мыши + таблица биндингов.
//! Сохраняется в settings.json, читается обоими бинарниками.

use bevy::prelude::*;

use crate::player::MouseSensitivity;
use crate::settings::Settings;

/// Ресурс: полный Settings, доступный UI.
#[derive(Resource, Debug, Clone)]
pub struct SettingsResource(pub Settings);

impl Default for SettingsResource {
    fn default() -> Self {
        SettingsResource(Settings::default())
    }
}

/// Маркер: корневая панель настроек.
#[derive(Component)]
pub struct SettingsPanel;

/// Компонент: слайдер чувствительности.
#[derive(Component, Debug, Clone, Copy)]
pub struct SensitivitySlider;

/// Компонент кнопки изменения биндинга.
#[derive(Component, Debug, Clone)]
pub struct BindingRow {
    pub action: String,
}

/// Система: системные шаги слайдера чувствительности стрелками влево/вправо.
pub fn sensitivity_slider_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sr: ResMut<SettingsResource>,
    mut sens: ResMut<MouseSensitivity>,
) {
    let mut changed = false;
    if keys.just_pressed(KeyCode::ArrowRight) {
        sr.0.mouse_sensitivity = (sr.0.mouse_sensitivity + 0.1).min(5.0);
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        sr.0.mouse_sensitivity = (sr.0.mouse_sensitivity - 0.1).max(0.1);
        changed = true;
    }
    if changed {
        sens.0 = sr.0.mouse_sensitivity;
    }
}

/// Обработка кликов по строкам биндингов (заглушка: смена на следующую клавишу).
pub fn binding_row_click(
    interactions: Query<(&Interaction, &BindingRow), Changed<Interaction>>,
    mut sr: ResMut<SettingsResource>,
) {
    for (interaction, row) in interactions.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Простая ротация по циклу клавиш для демонстрации.
        let next = match sr.0.key_for(&row.action).as_str() {
            "W" => "Up",
            "Up" => "W",
            _ => "W",
        };
        sr.0.bindings.insert(row.action.clone(), next.to_string());
    }
}

/// Система: сохранение Settings в settings.json при изменениях.
pub fn save_settings_on_change(
    sr: Res<SettingsResource>,
    paths: Option<Res<crate::paths::AppPathsResource>>,
) {
    if !sr.is_changed() {
        return;
    }
    if let Some(p) = paths {
        let _ = sr.0.save(&p.0.settings_json);
    }
}

/// Панель настроек: заглушечный spawn.
pub fn spawn_settings_panel(mut commands: Commands) {
    commands
        .spawn((
            NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
                visibility: Visibility::Hidden,
                ..default()
            },
            SettingsPanel,
        ))
        .with_children(|col| {
            col.spawn(TextBundle::from_section(
                "Чувствительность мыши (стрелки влево/вправо)",
                TextStyle {
                    font_size: 18.0,
                    color: Color::WHITE,
                    ..default()
                },
            ));
            for action in [
                "move_forward",
                "move_back",
                "move_left",
                "move_right",
                "sprint",
                "crouch",
                "jump",
                "interact",
                "pause",
            ] {
                col.spawn((
                    ButtonBundle {
                        style: Style {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                            ..default()
                        },
                        background_color: BackgroundColor(Color::srgb(0.2, 0.2, 0.25)),
                        ..default()
                    },
                    BindingRow {
                        action: action.to_string(),
                    },
                ))
                .with_children(|b| {
                    b.spawn(TextBundle::from_section(
                        action,
                        TextStyle {
                            font_size: 16.0,
                            color: Color::WHITE,
                            ..default()
                        },
                    ));
                });
            }
        });
}

/// Плагин панели настроек.
#[derive(Default)]
pub struct SettingsPanelPlugin;

impl Plugin for SettingsPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettingsResource>()
            .add_systems(Startup, spawn_settings_panel)
            .add_systems(
                Update,
                (sensitivity_slider_input, binding_row_click, save_settings_on_change),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitivity_clamped() {
        let mut s = Settings::default();
        s.mouse_sensitivity = 10.0;
        s.clamp_sensitivity();
        assert!((s.mouse_sensitivity - 5.0).abs() < 1e-6);
    }

    #[test]
    fn plugin_builds() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<Settings>();
        app.init_resource::<crate::player::MouseSensitivity>();
        app.add_plugins(SettingsPanelPlugin);
        app.update();
    }
}