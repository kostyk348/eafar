#pragma once
// eafar/tests/test_runner.hpp — dependency-free test harness.
// No external test framework: keeps the library self-contained.

#include <cstdio>
#include <exception>
#include <stdexcept>
#include <string>
#include <vector>

namespace eafar::test {

struct TestCase {
    const char* name;
    void (*fn)();
};

inline std::vector<TestCase>& registry() {
    static std::vector<TestCase> r;
    return r;
}

struct Registrar {
    Registrar(const char* name, void (*fn)()) {
        registry().push_back(TestCase{name, fn});
    }
};

inline int run_all() {
    int failed = 0;
    for (auto& t : registry()) {
        try {
            t.fn();
            std::printf("[PASS] %s\n", t.name);
        } catch (const std::exception& e) {
            ++failed;
            std::printf("[FAIL] %s: %s\n", t.name, e.what());
        } catch (...) {
            ++failed;
            std::printf("[FAIL] %s: unknown exception\n", t.name);
        }
    }
    std::printf("%zu tests, %d failed\n", registry().size(), failed);
    return failed == 0 ? 0 : 1;
}

} // namespace eafar::test

#define EAFAR_STRINGIFY_IMPL(x) #x
#define EAFAR_STRINGIFY(x) EAFAR_STRINGIFY_IMPL(x)

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            throw std::runtime_error("CHECK failed: " #cond " at " __FILE__    \
                                     ":" EAFAR_STRINGIFY(__LINE__));           \
        }                                                                      \
    } while (0)

#define CHECK_EQ(a, b)                                                         \
    do {                                                                       \
        auto va = (a);                                                         \
        auto vb = (b);                                                         \
        if (!(va == vb)) {                                                     \
            throw std::runtime_error("CHECK_EQ failed: " #a " == " #b " at "   \
                                     __FILE__ ":" EAFAR_STRINGIFY(__LINE__));  \
        }                                                                      \
    } while (0)

#define CHECK_THROWS(expr)                                                     \
    do {                                                                       \
        bool threw = false;                                                    \
        try {                                                                  \
            (void)(expr);                                                      \
        } catch (...) {                                                        \
            threw = true;                                                      \
        }                                                                      \
        if (!threw) {                                                          \
            throw std::runtime_error("CHECK_THROWS failed: " #expr " at "      \
                                     __FILE__ ":" EAFAR_STRINGIFY(__LINE__));  \
        }                                                                      \
    } while (0)

// Float compare with tolerance (accumulation drift aware).
#define CHECK_NEAR(a, b, eps)                                                  \
    do {                                                                       \
        auto va = (a);                                                         \
        auto vb = (b);                                                         \
        auto delta = (va > vb) ? (va - vb) : (vb - va);                        \
        if (!(delta <= (eps))) {                                               \
            throw std::runtime_error("CHECK_NEAR failed: " #a " ~= " #b " at " \
                                     __FILE__ ":" EAFAR_STRINGIFY(__LINE__));  \
        }                                                                      \
    } while (0)

#define TEST(name)                                                             \
    static void test_##name();                                                 \
    static ::eafar::test::Registrar reg_##name(#name, &test_##name);           \
    static void test_##name()
