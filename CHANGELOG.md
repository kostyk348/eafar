# Changelog — EAFAR

Все заметные изменения фиксируются здесь. Формат — упрощённый [Keep a Changelog](https://keepachangelog.com/).

## v1.0.0 — переписывание на Rust (2026-10-30)

`main` теперь чистый Rust-крейт; реализация на C++23 живёт в истории git.

### Добавлено

- крейт `eafar` 1.0.0 (edition 2021): ноль зависимостей, `#![forbid(unsafe_code)]`;
- модули S1–S10: `field_map`, `event_queue`, `region_scheduler`, `automaton`, `fuzzy`,
  `dependency`, `diffusion`, `snapshot`, `rng`, `visualizer` + `prelude`;
- 78 контрактных тестов (C++-набор — 72+) перенесён и расширен, включая:
  референсные векторы SplitMix64 (бит-в-бит), бит-точный откат снапшотов
  (паттерны NaN, `-0.0`), реплей-идентичность после отката;
- бенчмарк перенесён в `src/bin/eafar-bench.rs` (std::time::Instant, без criterion);
- пример `examples/heatmap.rs` — картинка разреженного мира с обводкой активных регионов;
- CI на cargo (build + test, неблокирующий cross-check на Windows/macOS).

### Изменено (семантика сохранена, уровень API)

- исключения → `Result<T, Error>`; тексты ошибок сохранены;
- `EventType::_count` и пустые membership-состояния убраны (в Rust непредставимы);
- `AutomatonManager::at` возвращает `Option` вместо исключения;
- `RegionWork` принимает явный `&mut RegionScheduler` (в C++ замыкание захватило по ссылке);
- дефолтный аргумент `payload = 1.0f` убран — все вызовы передают его явно;
- CSV-писатель воспроизводит формат `%g` (6 значащих цифр) `ostream<<`.

### Гарантии детерминизма (перенесены и протестированы)

SplitMix64 бит-в-бит с C++; снапшоты бит-точные; обход — по возрастанию id (`BTreeSet`);
порядок float-операций в диффузии совпадает с C++ (тот же порядок соседей ядра).

## v0.1.0 — first snapshot (2026-10-16)

- библиотека C++23: S1–S10 (FieldMap, EventQueue, RegionScheduler, AutomatonManager,
  FuzzyState, DependencyGraph, DiffusionEngine, WorldSnapshot, RNG, Visualizer);
- 72+ контрактных теста, бенчмарк разреженности (~сотни× на 0.1% активных регионов);
- сборка: CMake ≥ 3.20 (GCC 13+/Clang 16+/MSVC 17.6+), скрипт MinGW `build.ps1`.
