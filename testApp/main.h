//
// Created by pat on 3/8/23.
//

#ifndef BURN_MAIN_H
#define BURN_MAIN_H
#include <iostream>
#include <folly/init/Init.h>
#include <folly/experimental/coro/Task.h>
#include <folly/experimental/coro/BlockingWait.h>
#include <folly/experimental/coro/AsyncScope.h>
#include <folly/experimental/coro/Mutex.h>
#include <gflags/gflags.h>
#include <folly/Conv.h>
#include <folly/logging/xlog.h>
#include <folly/logging/LogLevel.h>
#include <chrono>
#include <folly/concurrency/ConcurrentHashMap.h>
#include <folly/Random.h>
#include <csignal>
#include <cstdlib>
#include <unistd.h>

folly::coro::Task<void> map_write(std::string k, std::string v);
folly::coro::Task<void> map_clear();
folly::coro::Task<uint64_t> map_size();
folly::coro::Task<void> burn_cycles(auto start);
folly::coro::Task<void> run();
void signal_callback_handler(int signum);

#endif //BURN_MAIN_H
