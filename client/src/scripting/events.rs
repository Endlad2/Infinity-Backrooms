//! Событийная шина между движком (Bevy) и Lua-скриптами.
//! Определяет типы хуков и очередь отложенных действий (например, api.timer.after).

use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookKind {
    OnSpawn,
    OnUpdate,
    OnCollide,
    OnDeath,
    OnPickup,
    OnOpen,
    OnClose,
    OnLevelStart,
    OnLevelEnd,
    OnEnter,
    OnExit,
    OnTick,
    OnSignal,
}

impl HookKind {
    /// Имя функции в Lua (совпадает с видом хука).
    pub fn lua_name(&self) -> &'static str {
        match self {
            HookKind::OnSpawn => "on_spawn",
            HookKind::OnUpdate => "on_update",
            HookKind::OnCollide => "on_collide",
            HookKind::OnDeath => "on_death",
            HookKind::OnPickup => "on_pickup",
            HookKind::OnOpen => "on_open",
            HookKind::OnClose => "on_close",
            HookKind::OnLevelStart => "on_level_start",
            HookKind::OnLevelEnd => "on_level_end",
            HookKind::OnEnter => "on_enter",
            HookKind::OnExit => "on_exit",
            HookKind::OnTick => "on_tick",
            HookKind::OnSignal => "on_signal",
        }
    }

    /// Парсит имя узла-события из XML (<on_spawn .../>) в HookKind.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "on_spawn" => HookKind::OnSpawn,
            "on_update" => HookKind::OnUpdate,
            "on_collide" => HookKind::OnCollide,
            "on_death" => HookKind::OnDeath,
            "on_pickup" => HookKind::OnPickup,
            "on_open" => HookKind::OnOpen,
            "on_close" => HookKind::OnClose,
            "on_level_start" => HookKind::OnLevelStart,
            "on_level_end" => HookKind::OnLevelEnd,
            "on_enter" => HookKind::OnEnter,
            "on_exit" => HookKind::OnExit,
            "on_tick" => HookKind::OnTick,
            "on_signal" => HookKind::OnSignal,
            _ => return None,
        })
    }
}

/// Запись о привязке <on_spawn script="enemy_ai" func="on_spawn"/>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCall {
    pub script_id: String,
    pub func: String,
}

/// Отложенный вызов: `api.timer.after(sec, function() ... end)`.
#[derive(Debug, Clone)]
pub struct DeferredCall {
    pub remaining: f32,
    /// Тело функции в Lua (строка, переданная в api.timer.after).
    pub lua_body: String,
}

/// Очередь отложенных вызовов и общий рантайм-лог.
#[derive(Debug, Default)]
pub struct DeferredQueue {
    queue: VecDeque<DeferredCall>,
}

impl DeferredQueue {
    pub fn push(&mut self, call: DeferredCall) {
        self.queue.push_back(call);
    }

    /// Тикаем `dt` секунд, возвращаем Lua-тела, которые пора выполнить.
    pub fn tick(&mut self, dt: f32) -> Vec<String> {
        let mut ready = Vec::new();
        let mut keep = VecDeque::new();
        while let Some(mut c) = self.queue.pop_front() {
            c.remaining -= dt;
            if c.remaining <= 0.0 {
                ready.push(c.lua_body);
            } else {
                keep.push_back(c);
            }
        }
        self.queue = keep;
        ready
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hooks() {
        assert_eq!(HookKind::parse("on_spawn"), Some(HookKind::OnSpawn));
        assert_eq!(HookKind::parse("on_level_start"), Some(HookKind::OnLevelStart));
        assert_eq!(HookKind::parse("nope"), None);
        assert_eq!(HookKind::OnUpdate.lua_name(), "on_update");
    }

    #[test]
    fn deferred_queue_fires_after_time() {
        let mut q = DeferredQueue::default();
        q.push(DeferredCall { remaining: 1.0, lua_body: "print('a')".into() });
        q.push(DeferredCall { remaining: 2.0, lua_body: "print('b')".into() });

        assert!(q.tick(0.5).is_empty());
        let ready = q.tick(0.6); // прошло 1.1 → должно выстрелить 'a'
        assert_eq!(ready, vec!["print('a')".to_string()]);
        assert_eq!(q.len(), 1);

        let ready2 = q.tick(1.0);
        assert_eq!(ready2, vec!["print('b')".to_string()]);
        assert!(q.is_empty());
    }
}