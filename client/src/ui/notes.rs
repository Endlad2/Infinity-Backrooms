//! Меню «Добавить замечание ИИ» (§7.3 ТЗ).
//! Пользователь вводит текст, который добавляется в промт перегенерации:
//!   Замечания от прошлой генерации - {текст}
//! После этого уровень пересоздаётся и сохраняется в levels/{N}.xml.

use bevy::prelude::*;

use crate::game::ActiveScene;
use crate::paths::AppPathsResource;

/// Маркер корневой панели замечаний.
#[derive(Component)]
pub struct NotesPanel;

/// Ресурс: текущее содержимое текстового поля.
#[derive(Resource, Default)]
pub struct NotesBuffer {
    pub text: String,
}

/// Ресурс: последнее отправленное замечание (используется main-циклом для перегенерации).
#[derive(Resource, Default)]
pub struct PendingNotes {
    pub pending: Option<String>,
}

/// Система: сбор символов с клавиатуры в буфер NotesBuffer.
pub fn notes_text_input(
    mut ev: EventReader<bevy::input::keyboard::KeyboardInput>,
    mut buf: ResMut<NotesBuffer>,
) {
    for e in ev.read() {
        if !e.state.is_pressed() {
            continue;
        }
        match e.key_code {
            KeyCode::Backspace => {
                buf.text.pop();
            }
            KeyCode::Enter => {
                // Отправка — обрабатывается системой submit_notes.
            }
            KeyCode::Space => buf.text.push(' '),
            KeyCode::KeyA => buf.text.push('a'),
            KeyCode::KeyB => buf.text.push('b'),
            KeyCode::KeyC => buf.text.push('c'),
            KeyCode::KeyD => buf.text.push('d'),
            KeyCode::KeyE => buf.text.push('e'),
            KeyCode::KeyF => buf.text.push('f'),
            KeyCode::KeyG => buf.text.push('g'),
            KeyCode::KeyH => buf.text.push('h'),
            KeyCode::KeyI => buf.text.push('i'),
            KeyCode::KeyJ => buf.text.push('j'),
            KeyCode::KeyK => buf.text.push('k'),
            KeyCode::KeyL => buf.text.push('l'),
            KeyCode::KeyM => buf.text.push('m'),
            KeyCode::KeyN => buf.text.push('n'),
            KeyCode::KeyO => buf.text.push('o'),
            KeyCode::KeyP => buf.text.push('p'),
            KeyCode::KeyQ => buf.text.push('q'),
            KeyCode::KeyR => buf.text.push('r'),
            KeyCode::KeyS => buf.text.push('s'),
            KeyCode::KeyT => buf.text.push('t'),
            KeyCode::KeyU => buf.text.push('u'),
            KeyCode::KeyV => buf.text.push('v'),
            KeyCode::KeyW => buf.text.push('w'),
            KeyCode::KeyX => buf.text.push('x'),
            KeyCode::KeyY => buf.text.push('y'),
            KeyCode::KeyZ => buf.text.push('z'),
            _ => {}
        }
    }
}

/// Система: Enter переводит буфер в PendingNotes.
pub fn submit_notes(
    keys: Res<ButtonInput<KeyCode>>,
    mut buf: ResMut<NotesBuffer>,
    mut pending: ResMut<PendingNotes>,
) {
    if keys.just_pressed(KeyCode::Enter) && !buf.text.trim().is_empty() {
        pending.pending = Some(buf.text.clone());
        buf.text.clear();
    }
}

/// Плагин замечаний.
#[derive(Default)]
pub struct NotesPlugin;

impl Plugin for NotesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotesBuffer>()
            .init_resource::<PendingNotes>()
            .add_systems(Startup, spawn_notes_panel)
            .add_systems(Update, (notes_text_input, submit_notes));
    }
}

/// Заглушечный спавн панели (в реальности — позиционируется поверх оверлея паузы).
pub fn spawn_notes_panel(mut commands: Commands) {
    commands.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Percent(20.0),
                top: Val::Percent(40.0),
                width: Val::Percent(60.0),
                padding: UiRect::all(Val::Px(16.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            background_color: BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            visibility: Visibility::Hidden,
            ..default()
        },
        NotesPanel,
    ))
    .with_children(|c| {
        c.spawn(TextBundle::from_section(
            "Замечание для ИИ (Enter — отправить, Backspace — стереть):",
            TextStyle {
                font_size: 16.0,
                color: Color::WHITE,
                ..default()
            },
        ));
    });
}

/// Проверяет наличие pending замечания и вызывает перегенерацию уровня.
/// Возвращает строку, если замечание было отправлено (для main-цикла).
pub fn take_pending(pending: &mut PendingNotes) -> Option<String> {
    pending.pending.take()
}

/// Вспомогательная функция: путь XML уровня по номеру.
pub fn level_xml_path(level_number: u32) -> Option<std::path::PathBuf> {
    let paths = AppPathsResource::default();
    Some(paths.0.level_xml(level_number))
}

/// Вспомогательная функция: номер активного уровня.
pub fn active_level_number(scene: &ActiveScene) -> u32 {
    scene.level_number
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_accepts_text() {
        let mut b = NotesBuffer::default();
        b.text.push_str("hello");
        assert_eq!(b.text, "hello");
    }

    #[test]
    fn plugin_builds() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.add_plugins(NotesPlugin);
        app.update();
    }
}