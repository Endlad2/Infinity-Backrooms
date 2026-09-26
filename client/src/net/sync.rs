//! Синхронизация мира между хостом и клиентом (§5.4 ТЗ).
//! Хост — авторитетная сторона. Клиент интерполирует удалённых игроков,
//! локального игрока ведёт сам (client-side prediction c мягкой реконсиляцией).

use std::collections::HashMap;

use super::protocol::{PlayerState, WorldSnapshot};

/// Настройки интерполяции.
#[derive(Debug, Clone, Copy)]
pub struct SyncConfig {
    /// Задержка интерполяции (сек) — держим снапшоты немного в прошлом.
    pub interp_delay: f32,
    /// Максимальное расстояние, при котором локального игрока плавно тянет к серверу.
    pub reconcile_snap_distance: f32,
    /// Коэффициент сглаживания (0..1). Чем больше — тем быстрее догоняем.
    pub reconcile_lerp: f32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            interp_delay: 0.1,
            reconcile_snap_distance: 3.0,
            reconcile_lerp: 0.15,
        }
    }
}

/// Один удалённый игрок с буфером снапшотов для интерполяции.
#[derive(Debug, Clone)]
pub struct RemotePlayer {
    pub id: u32,
    pub prev: PlayerState,
    pub next: PlayerState,
    pub t: f32,
}

impl RemotePlayer {
    fn new(state: PlayerState) -> Self {
        RemotePlayer {
            id: state.id,
            prev: state.clone(),
            next: state,
            t: 1.0,
        }
    }

    /// Продвигает буфер: prev <- интерполированное, next <- новое.
    fn push(&mut self, state: PlayerState, step: f32) {
        let mid = self.sample_at(1.0);
        self.prev = mid;
        self.next = state;
        self.t = 0.0;
        let _ = step;
    }

    /// Продвигает t на dt и возвращает интерполированное состояние.
    pub fn advance(&mut self, dt: f32, cfg: SyncConfig) -> PlayerState {
        if cfg.interp_delay > 0.0 {
            self.t += dt / cfg.interp_delay;
            if self.t > 1.0 {
                self.t = 1.0;
            }
        } else {
            self.t = 1.0;
        }
        self.sample_at(self.t)
    }

    /// Интерполирует позицию между prev и next с параметром t (0..1).
    pub fn sample_at(&self, t: f32) -> PlayerState {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        let mut s = self.next.clone();
        s.x = lerp(self.prev.x, self.next.x);
        s.y = lerp(self.prev.y, self.next.y);
        s.z = lerp(self.prev.z, self.next.z);
        s.yaw = lerp_angle(self.prev.yaw, self.next.yaw, t);
        s.pitch = lerp(self.prev.pitch, self.next.pitch);
        s.hp = self.next.hp;
        s.flags = self.next.flags;
        s
    }
}

/// Интерполяция углов с учётом перехода через 360.
fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut delta = (b - a) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    a + delta * t
}

/// Клиентский интерполятор мира.
#[derive(Debug, Default)]
pub struct WorldInterpolator {
    pub remotes: HashMap<u32, RemotePlayer>,
    pub local_player_id: Option<u32>,
    pub last_tick: u32,
    pub config: SyncConfig,
    /// Накопленная ошибка реконсиляции локального игрока (для отладки/UI).
    pub last_reconcile_error: f32,
}

impl WorldInterpolator {
    pub fn new(cfg: SyncConfig) -> Self {
        WorldInterpolator {
            remotes: HashMap::new(),
            local_player_id: None,
            last_tick: 0,
            config: cfg,
            last_reconcile_error: 0.0,
        }
    }

    /// Устанавливает id локального игрока.
    pub fn set_local(&mut self, id: u32) {
        self.local_player_id = Some(id);
    }

    /// Применяет пришедший снапшот: обновляет буферы удалённых игроков,
    /// возвращает авторитетное состояние локального игрока (если есть).
    pub fn apply_snapshot(&mut self, snap: &WorldSnapshot) -> Option<PlayerState> {
        self.last_tick = snap.tick;
        let mut local_auth: Option<PlayerState> = None;
        for p in &snap.players {
            if Some(p.id) == self.local_player_id {
                local_auth = Some(p.clone());
                continue;
            }
            match self.remotes.get_mut(&p.id) {
                Some(rp) => rp.push(p.clone(), 1.0),
                None => {
                    self.remotes.insert(p.id, RemotePlayer::new(p.clone()));
                }
            }
        }
        local_auth
    }

    /// Продвигает интерполяцию всех удалённых игроков на dt.
    pub fn advance(&mut self, dt: f32) -> Vec<PlayerState> {
        self.remotes
            .values_mut()
            .map(|rp| rp.advance(dt, self.config))
            .collect()
    }

    /// Мягкая реконсиляция локального игрока: плавно тянем предсказанную позицию к авторитетной.
    /// Возвращает скорректированную позицию (x, y, z).
    pub fn reconcile_local(
        &mut self,
        predicted: (f32, f32, f32),
        authoritative: &PlayerState,
    ) -> (f32, f32, f32) {
        let dx = authoritative.x - predicted.0;
        let dy = authoritative.y - predicted.1;
        let dz = authoritative.z - predicted.2;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        self.last_reconcile_error = dist;

        // Слишком большое расхождение — жёсткий snap (телепорт/лаг).
        if dist > self.config.reconcile_snap_distance {
            return (authoritative.x, authoritative.y, authoritative.z);
        }

        let k = self.config.reconcile_lerp.clamp(0.0, 1.0);
        (
            predicted.0 + dx * k,
            predicted.1 + dy * k,
            predicted.2 + dz * k,
        )
    }

    /// Убирает удалённых игроков, которых нет в новом снапшоте (для простоты — только явно).
    pub fn remove_remote(&mut self, id: u32) {
        self.remotes.remove(&id);
    }

    /// Кол-во удалённых игроков.
    pub fn remote_count(&self) -> usize {
        self.remotes.len()
    }
}

/// Собирает снапшот из локального игрока и удалённых (используется хостом).
pub fn build_snapshot(
    tick: u32,
    local: &PlayerState,
    remotes: &[PlayerState],
) -> WorldSnapshot {
    let mut snap = WorldSnapshot::new(tick);
    snap.players.push(local.clone());
    snap.players.extend_from_slice(remotes);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ps(id: u32, x: f32, z: f32, yaw: f32) -> PlayerState {
        let mut p = PlayerState::new(id);
        p.x = x;
        p.z = z;
        p.yaw = yaw;
        p
    }

    #[test]
    fn interpolates_position() {
        let mut rp = RemotePlayer::new(ps(1, 0.0, 0.0, 0.0));
        rp.push(ps(1, 10.0, 0.0, 0.0), 1.0);
        let mid = rp.sample_at(0.5);
        assert!((mid.x - 5.0).abs() < 1e-4);
    }

    #[test]
    fn interpolates_angle_across_wrap() {
        // 350 -> 10 должно идти через 0, а не назад.
        let mid = lerp_angle(350.0, 10.0, 0.5);
        // Ожидаем ~0 (или 360), не 180.
        let normalized = if mid > 180.0 { mid - 360.0 } else { mid };
        assert!(normalized.abs() < 1e-3, "got {mid}");
    }

    #[test]
    fn apply_snapshot_creates_remotes() {
        let mut interp = WorldInterpolator::new(SyncConfig::default());
        interp.set_local(1);
        let mut snap = WorldSnapshot::new(5);
        snap.players.push(ps(1, 1.0, 1.0, 0.0)); // local
        snap.players.push(ps(2, 5.0, 5.0, 90.0)); // remote
        let auth = interp.apply_snapshot(&snap);
        assert!(auth.is_some());
        assert_eq!(interp.remote_count(), 1);
        assert_eq!(interp.last_tick, 5);
    }

    #[test]
    fn reconcile_snaps_on_large_error() {
        let mut interp = WorldInterpolator::new(SyncConfig::default());
        let auth = ps(1, 100.0, 0.0, 0.0);
        let out = interp.reconcile_local((0.0, 0.0, 0.0), &auth);
        assert!((out.0 - 100.0).abs() < 1e-4);
        assert!(interp.last_reconcile_error > 3.0);
    }

    #[test]
    fn reconcile_lerps_on_small_error() {
        let mut interp = WorldInterpolator::new(SyncConfig::default());
        let auth = ps(1, 1.0, 0.0, 0.0);
        let out = interp.reconcile_local((0.0, 0.0, 0.0), &auth);
        // мягкая коррекция: значение между 0 и 1.
        assert!(out.0 > 0.0 && out.0 < 1.0);
    }

    #[test]
    fn build_snapshot_includes_all() {
        let local = ps(1, 0.0, 0.0, 0.0);
        let remotes = vec![ps(2, 1.0, 1.0, 0.0), ps(3, 2.0, 2.0, 0.0)];
        let snap = build_snapshot(42, &local, &remotes);
        assert_eq!(snap.tick, 42);
        assert_eq!(snap.players.len(), 3);
    }

    #[test]
    fn advance_moves_interpolation() {
        let mut interp = WorldInterpolator::new(SyncConfig {
            interp_delay: 0.1,
            ..Default::default()
        });
        interp.set_local(1);
        let mut snap = WorldSnapshot::new(1);
        snap.players.push(ps(1, 0.0, 0.0, 0.0));
        snap.players.push(ps(2, 0.0, 0.0, 0.0));
        interp.apply_snapshot(&snap);
        let mut snap2 = WorldSnapshot::new(2);
        snap2.players.push(ps(1, 0.0, 0.0, 0.0));
        snap2.players.push(ps(2, 10.0, 0.0, 0.0));
        interp.apply_snapshot(&snap2);
        let states = interp.advance(0.05);
        assert_eq!(states.len(), 1);
        // Значение должно быть между 0 и 10.
        assert!(states[0].x > 0.0 && states[0].x < 10.0);
    }
}