//! Модуль ассетов:
//!   * `polyhaven`  — клиент Poly Haven API (search / files / info / download).
//!   * `stage1`     — Этап 1: ИИ решает, что нужно, Poly Haven скачивает, кэш в cache-files/.
//!   * `db`         — SQLite-кэш текстур/моделей (используется как вторичный).
//!   * `svg`        — SVG→PNG растеризация (для ИИ-заглушек).
//!   * `gen`        — локальная ИИ-генерация ассетов (fallback).
//!   * `resolver`   — высокоуровневый резолвер (cache-files → assets.db → ИИ).

pub mod polyhaven;
pub mod stage1;
pub mod db;
pub mod svg;
pub mod gen;
pub mod resolver;
