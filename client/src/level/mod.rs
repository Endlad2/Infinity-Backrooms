//! BDS Level Format v1: модель, парсер XML, построение сцены, мост к ассетам,
//! а также оркестратор генерации уровня через ИИ.

pub mod model;
pub mod parse;
pub mod build;
pub mod assets_bridge;
pub mod gen;