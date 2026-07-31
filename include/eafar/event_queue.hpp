#pragma once
// eafar/event_queue.hpp — S2: Event Queue.
//
// The world changes ONLY through events. A handler reacts to an event by
// writing fields and/or pushing follow-up events (cascade). New events are
// appended to the tail and processed in strict FIFO order — deterministic
// by construction (no timers, no unordered containers, no wall-clock).
//
// Handlers see only {world fields, queue} — never other handlers.
// This is the coupling rule that S6 (Dependency Graph) formalizes.

#include "eafar/event.hpp"

#include <array>
#include <cstdint>
#include <deque>
#include <functional>
#include <vector>

namespace eafar {

class EventQueue;
class FieldMap;

using EventHandler = std::function<void(const Event&, FieldMap&, EventQueue&)>;

class EventQueue {
public:
    void push(EventType type, std::uint32_t x, std::uint32_t y,
              float payload = 1.0f);

    void subscribe(EventType type, EventHandler handler);
    void clear_handlers() noexcept;

    // Process events until the queue is empty. Events pushed by handlers
    // during processing are appended and processed in the same drain —
    // strict FIFO, deterministic order.
    void process_all(FieldMap& world);

    bool empty() const noexcept { return queue_.empty(); }
    std::size_t pending_count() const noexcept { return queue_.size(); }
    std::uint64_t processed_count() const noexcept { return processed_; }
    std::uint64_t next_seq() const noexcept { return next_seq_; }

    // Append-only log of every processed event (S9 replay / determinism).
    const std::vector<Event>& history() const noexcept { return history_; }

private:
    std::deque<Event> queue_;
    std::array<std::vector<EventHandler>, kEventTypeCount> handlers_;
    std::vector<Event> history_;
    std::uint64_t next_seq_ = 0;
    std::uint64_t processed_ = 0;
};

} // namespace eafar
