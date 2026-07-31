cmake_minimum_required(VERSION 3.25)
project(eafar VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

# Options so the core can be consumed as a dependency without building
# its own tests/benchmarks (used by eafar-db etc.).
option(EAFAR_BUILD_TESTS "Build EAFAR test suite" ON)
option(EAFAR_BUILD_BENCH "Build EAFAR benchmarks" ON)

# --- EAFAR: Event-Driven Asynchronous Fuzzy Automata Runtime ---
add_library(eafar STATIC
    src/field_map.cpp
    src/event_queue.cpp
    src/region_scheduler.cpp
    src/automaton.cpp
    src/dependency.cpp
    src/diffusion.cpp
    src/visualizer.cpp
)
target_include_directories(eafar PUBLIC include)
target_compile_options(eafar PRIVATE -Wall -Wextra)

if(EAFAR_BUILD_TESTS)
  # --- Tests ---
  add_executable(eafar_tests
      tests/main.cpp
      tests/test_field_map.cpp
      tests/test_event_queue.cpp
      tests/test_region_scheduler.cpp
      tests/test_automaton.cpp
      tests/test_fuzzy.cpp
      tests/test_dependency.cpp
      tests/test_diffusion.cpp
      tests/test_snapshot.cpp
      tests/test_replay.cpp
      tests/test_visualizer.cpp
  )
  target_link_libraries(eafar_tests PRIVATE eafar)
  target_compile_options(eafar_tests PRIVATE -Wall -Wextra)

  enable_testing()
  add_test(NAME eafar_tests COMMAND eafar_tests)
endif()

if(EAFAR_BUILD_BENCH)
  # --- Benchmarks ---
  add_executable(eafar_bench
      benchmarks/bench_main.cpp
  )
  target_link_libraries(eafar_bench PRIVATE eafar)
  target_compile_options(eafar_bench PRIVATE -Wall -Wextra)
endif()
