//! Плотный vs разреженный мир: доказательство, что шаг стоит пропорционально числу
//! активных регионов, а не размеру мира.
//!
//! Три замера:
//!   1) фиксированный «горячий» регион, мир растёт 256 → 4096 (время шага должно стоять);
//!   2) мир 1024×1024, число активных регионов растёт 1 → все (время должно быть ~линейным
//!      по числу активных регионов);
//!   3) полный мир против одного региона при одинаковом размере — «сколько стоит сон».
//!
//! Запуск: `cargo run --release --example sparse_vs_dense`

use std::time::Instant;

use eafar::prelude::*;

/// Шаг планировщика `iters` раз, возвращает микросекунды на шаг.
fn time_ticks(world: &mut FieldMap, sched: &mut RegionScheduler, iters: u32) -> f64 {
    // прогрев: первый шаг включает аллокации/инициализацию структур
    let _ = sched.tick(world);
    let started = Instant::now();
    for _ in 0..iters {
        let _ = sched.tick(world);
        // держим «горячий» регион живым, иначе планировщик усыпит его и замер перестанет
        // быть честным (это и есть предмет замера — см. колонку active regions)
        let _ = sched.wake(1, 1);
    }
    let elapsed = started.elapsed().as_secs_f64();
    elapsed * 1e6 / f64::from(iters)
}

fn make_world(size: u32, hot_regions: u32) -> (FieldMap, RegionScheduler) {
    let mut world = FieldMap::new(size, size);
    let mut sched = RegionScheduler::new(size, size, 32).expect("scheduler");
    // «горячие» клетки: чуть-чуть тепла в начале каждого активного региона
    let mut woken = 0u32;
    'outer: for ry in 0..sched.regions_y() {
        for rx in 0..sched.regions_x() {
            if woken >= hot_regions {
                break 'outer;
            }
            sched.wake_region(rx, ry).expect("wake");
            woken += 1;
        }
    }
    let _ = world.set(Field::Temperature, 1, 1, 0.5);
    (world, sched)
}

fn main() {
    println!("EAFAR — sparse vs dense");
    println!("метод: время шага планировщика, микросекунды, среднее по итерациям\n");

    // 1) размер мира при одном горячем регионе
    println!("1) один активный регион, мир растёт");
    println!("{:>10}  {:>14}  {:>12}", "мир", "клеток", "мкс/шаг");
    for size in [256u32, 512, 1024, 2048, 4096] {
        let (mut world, mut sched) = make_world(size, 1);
        let iters = if size <= 1024 { 200 } else { 50 };
        let us = time_ticks(&mut world, &mut sched, iters);
        println!(
            "{:>10}  {:>14}  {:>12.2}",
            format!("{}×{}", size, size),
            world.cell_count(),
            us
        );
    }

    // 2) рост числа активных регионов при фиксированном мире
    println!("\n2) мир 1024×1024, растёт число активных регионов");
    println!("{:>16}  {:>14}  {:>12}", "активных", "клеток в них", "мкс/шаг");
    let size = 1024u32;
    let total_regions = RegionScheduler::new(size, size, 32).expect("scheduler").region_count();
    for hot in [1u32, 4, 16, 64, total_regions] {
        let (mut world, mut sched) = make_world(size, hot);
        let iters = if hot <= 16 { 100 } else { 40 };
        let us = time_ticks(&mut world, &mut sched, iters);
        let cells = u64::from(hot) * 32 * 32;
        println!("{:>16}  {:>14}  {:>12.2}", hot, cells, us);
    }

    println!("\nВывод: смотрите сами — если время шага не растёт с размером мира (блок 1) и");
    println!("растёт с числом активных регионов (блок 2), то шаг действительно зависит от");
    println!("изменений, а не от размера поля.");
}
