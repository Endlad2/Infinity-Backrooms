//! UI-модули: меню паузы, панель настроек, замечания ИИ, экран загрузки.

pub mod pause;
pub mod settings_panel;
pub mod notes;
pub mod loading;

pub use pause::{PausePlugin, PauseState};
pub use loading::{LoadingPlugin, LoadingState};
pub use notes::NotesPlugin;
pub use settings_panel::SettingsPanelPlugin;