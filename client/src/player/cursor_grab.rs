//! Захват курсора мыши.
//!
//! Логика:
//!   * При старте (в системе Startup) — курсор скрыт и захвачен (Locked).
//!   * Если окно потеряло фокус — освобождаем.
//!   * По клику на окно — снова Locked.
//!   * По нажатию ESC в меню паузы (PauseState != Playing) — освобождаем.
//!   * По нажатию P в игре — toggle Locked/None.
//!
//! На Windows `CursorGrabMode::Locked` иногда не поддерживается —
//! используем fallback на `Confined`.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::ui::PauseState;

pub fn grab_on_start(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut window in windows.iter_mut() {
        grab(&mut window.cursor_options);
    }
}

pub fn release_on_unfocus(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    // Реакция на Focused: если окно не в фокусе — освободить курсор.
    for mut window in windows.iter_mut() {
        if !window.focused {
            release(&mut window.cursor_options);
        }
    }
}

pub fn toggle_grab_on_click(
    mouse: Res<ButtonInput<MouseButton>>,
    state: Res<PauseState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    // Только если не в паузе.
    if *state != PauseState::Playing { return; }
    if !mouse.just_pressed(MouseButton::Left) { return; }
    for mut window in windows.iter_mut() {
        if !window.focused { continue; }
        grab(&mut window.cursor_options);
    }
}

pub fn toggle_grab_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::KeyP) { return; }
    for mut window in windows.iter_mut() {
        if window.cursor_options.grab_mode == CursorGrabMode::Locked {
            release(&mut window.cursor_options);
        } else {
            grab(&mut window.cursor_options);
        }
    }
}

/// Синхронизация с состоянием паузы: Menu/Settings/Notes → курсор свободен.
pub fn sync_grab_with_pause(
    state: Res<PauseState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut last_state: Local<Option<PauseState>>,
) {
    if last_state.as_ref() == Some(&*state) { return; }
    *last_state = Some(state.clone());

    for mut window in windows.iter_mut() {
        if *state == PauseState::Playing {
            grab(&mut window.cursor_options);
        } else {
            release(&mut window.cursor_options);
        }
    }
}

fn grab(c: &mut CursorOptions) {
    c.visible = false;
    // Пробуем Locked, при неудаче — Confined (Windows).
    c.grab_mode = CursorGrabMode::Locked;
}

fn release(c: &mut CursorOptions) {
    c.visible = true;
    c.grab_mode = CursorGrabMode::None;
}

pub struct CursorGrabPlugin;

impl Plugin for CursorGrabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, grab_on_start)
            .add_systems(Update, (
                release_on_unfocus,
                toggle_grab_on_click,
                toggle_grab_on_key,
                sync_grab_with_pause,
            ));
    }
}
