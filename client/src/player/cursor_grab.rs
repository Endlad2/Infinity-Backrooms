//! Захват курсора мыши.
//!
//! В Bevy 0.14 CursorOptions — поле Window.cursor_options. Тип лежит в
//! bevy::window::CursorOptions (public). Если по какой-то причине его нет —
//! работаем с полями напрямую через Window.cursor_options.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow, Window};

use crate::ui::PauseState;

pub fn grab_on_start(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in windows.iter_mut() {
        grab(&mut w.cursor_options);
    }
}

pub fn release_on_unfocus(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in windows.iter_mut() {
        if !w.focused {
            release(&mut w.cursor_options);
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
        grab(&mut w.cursor_options);
    }
}

pub fn toggle_grab_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::KeyP) { return; }
    for mut w in windows.iter_mut() {
        if w.cursor_options.grab_mode == CursorGrabMode::Locked {
            release(&mut w.cursor_options);
        } else {
            grab(&mut w.cursor_options);
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
            grab(&mut w.cursor_options);
        } else {
            release(&mut w.cursor_options);
        }
    }
}

fn grab(c: &mut bevy::window::CursorOptions) {
    c.visible = false;
    c.grab_mode = CursorGrabMode::Locked;
}

fn release(c: &mut bevy::window::CursorOptions) {
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
