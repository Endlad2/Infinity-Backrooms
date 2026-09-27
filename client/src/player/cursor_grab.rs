//! Захват курсора мыши — БЕЗ упоминания типа поля `cursor`,
//! чтобы собираться на любой 0.14.x независимо от реэкспорта CursorOptions.
//!
//! Логика:
//!   * при старте — курсор скрыт и захвачен (Locked);
//!   * при потере фокуса — освобождаем;
//!   * клик по окну (в игре) — снова захват;
//!   * `P` — ручной toggle Locked/None;
//!   * при открытии меню паузы (ESC) — освобождаем.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow, Window};

use crate::ui::PauseState;

pub fn grab_on_start(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in windows.iter_mut() {
        grab(&mut w);
    }
}

pub fn release_on_unfocus(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in windows.iter_mut() {
        if !w.focused {
            release(&mut w);
        }
    }
}

pub fn toggle_grab_on_click(
    mouse: Res<ButtonInput<MouseButton>>,
    state: Res<PauseState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if *state != PauseState::Playing { return; }
    if !mouse.just_pressed(MouseButton::Left) { return; }
    for mut w in windows.iter_mut() {
        if !w.focused { continue; }
        grab(&mut w);
    }
}

pub fn toggle_grab_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::KeyP) { return; }
    for mut w in windows.iter_mut() {
        if w.cursor.grab_mode == CursorGrabMode::Locked {
            release(&mut w);
        } else {
            grab(&mut w);
        }
    }
}

pub fn sync_grab_with_pause(
    state: Res<PauseState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut last_state: Local<Option<PauseState>>,
) {
    if last_state.as_ref() == Some(&*state) { return; }
    *last_state = Some(state.clone());

    for mut w in windows.iter_mut() {
        if *state == PauseState::Playing {
            grab(&mut w);
        } else {
            release(&mut w);
        }
    }
}

/// Скрываем курсор и пытаемся захватить его в окне.
/// На Windows `Locked` иногда не поддерживается — Bevy сама сделает
/// fallback на `Confined`, если backend не умеет Locked.
fn grab(w: &mut Window) {
    w.cursor.visible = false;
    w.cursor.grab_mode = CursorGrabMode::Locked;
}

/// Показываем курсор и отпускаем его.
fn release(w: &mut Window) {
    w.cursor.visible = true;
    w.cursor.grab_mode = CursorGrabMode::None;
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
