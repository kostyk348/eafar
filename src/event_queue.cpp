#include "eafar/event_queue.hpp"

#include "eafar/field_map.hpp"

#include <stdexcept>

namespace eafar {

void EventQueue::push(EventType type, std::uint32_t x, std::uint32_t y,
                      float payload) {
    if (type == EventType::_count) {
        throw std::out_of_range("eafar: EventQueue::push invalid event type");
    }
    Event e{type, x, y, payload, next_seq_++};
    queue_.push_back(e);
}

void EventQueue::subscribe(EventType type, EventHandler handler) {
    if (type == EventType::_count) {
        throw std::out_of_range("eafar: EventQueue::subscribe invalid event type");
    }
    handlers_[static_cast<std::size_t>(type)].push_back(std::move(handler));
}

void EventQueue::clear_handlers() noexcept {
    for (auto& h : handlers_) {
        h.clear();
    }
}

void EventQueue::process_all(FieldMap& world) {
    while (!queue_.empty()) {
        Event e = queue_.front();
        queue_.pop_front();

        history_.push_back(e);
        ++processed_;

        for (auto& h : handlers_[static_cast<std::size_t>(e.type)]) {
            h(e, world, *this);
        }
    }
}

} // namespace eafar
