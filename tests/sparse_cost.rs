//! Регрессионный тест главного обещания EAFAR: стоимость тика зависит от числа активных
//! регионов, а не от площади мира.
//!
//! Замеры в README получены примером `examples/sparse_vs_dense.rs`. Здесь то же свойство
//! зафиксировано как проверка, чтобы будущее изменение не сломало его незаметно.
//!
//! Пороги намеренно щедрые: тест ловит не «стало на 20 % медленнее», а смену асимптотики
//! (когда тик начинает стоить пропорционально клеткам мира). Тайминги на CI шумные, поэтому
//! сравнение идёт по отношению двух замеров, сделанных подряд на одной машине.

use std::time::Instant;

use eafar::prelude::*;

/// Один шаг планировщика, микросекунды (лучший из трёх прогонов по `iters` тиков).
fn tick_micros(size: u32, iters: u32) -> f64 {
    tick_micros_with_wake(size, iters, false)
}

/// То же, но с явным выбором: будить все регионы на каждом тике или только один.
fn tick_micros_with_wake(size: u32, iters: u32, wake_all: bool) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..3 {
        let mut world = FieldMap::new(size, size);
        let mut sched = RegionScheduler::new(size, size, 32).expect("scheduler");
        let regions_x = sched.regions_x();
        let regions_y = sched.regions_y();
        let re_wake = |sched: &mut RegionScheduler| {
            if wake_all {
                for ry in 0..regions_y {
                    for rx in 0..regions_x {
                        let _ = sched.wake_region(rx, ry);
                    }
                }
            } else {
                let _ = sched.wake(1, 1);
            }
        };
        re_wake(&mut sched);
        let _ = world.set(Field::Temperature, 1, 1, 0.5);

        let _ = sched.tick(&mut world); // прогрев
        let started = Instant::now();
        for _ in 0..iters {
            let _ = sched.tick(&mut world);
            re_wake(&mut sched);
        }
        let us = started.elapsed().as_secs_f64() * 1e6 / f64::from(iters);
        if us < best {
            best = us;
        }
    }
    best
}

#[test]
fn tick_cost_does_not_scale_with_world_area() {
    // 256×256 (65 536 клеток) против 2048×2048 (4 194 304 клеток): площадь в 64 раза больше.
    let small = tick_micros(256, 400);
    let large = tick_micros(2048, 200);

    // Если бы тик стоил пропорционально площади, отношение было бы ≈ 64.
    // Допускаем 4× — это запас на шум CI и на кэш-эффекты, но не на смену асимптотики.
    let ratio = large / small;
    assert!(
        ratio < 4.0,
        "тик подорожал в {ratio:.2} раза при росте мира в 64 раза \
         (small={small:.3} µs, large={large:.3} µs) — похоже, стоимость снова зависит от площади"
    );
}

#[test]
fn active_regions_cost_more_than_one_region() {
    // Обе стороны замеряются одинаково: тик + повторное пробуждение региона(ов), иначе
    // регионы успевают уснуть по idle-таймауту и сравнение теряет смысл.
    let one = tick_micros_with_wake(1024, 300, false);
    let all = tick_micros_with_wake(1024, 100, true);

    assert!(
        all > one * 0.9,
        "полностью активный мир ({all:.3} µs) не дороже одного региона ({one:.3} µs) —          планировщик, похоже, игнорирует активные регионы"
    );
}
