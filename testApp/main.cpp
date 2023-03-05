#include <iostream>
#include <folly/init/Init.h>
#include <folly/experimental/coro/Task.h>
#include <folly/experimental/coro/BlockingWait.h>
#include <folly/experimental/coro/AsyncScope.h>
#include <gflags/gflags.h>
#include <folly/Conv.h>
#include <glog/logging.h>
#include <chrono>
#include <folly/concurrency/ConcurrentHashMap.h>
#include <folly/Random.h>

using namespace std;
using namespace folly;
using namespace chrono;
using namespace coro;

ConcurrentHashMap<string, string> bigChmOfStrings;

DEFINE_uint64(threads, 1, "Threads to waste (effectively).");
DEFINE_uint64(iterations, 100, "How much pointless stuff to do.");
DEFINE_uint64(mem_intensive_ratio, 50, "Do mem intensive stuff X % of time.");
DEFINE_uint64(dump_size, 100, "What capacity to clear map at.");

Task<void> burn_cycles() {
    string new_str;
    string old_str;
    std::ostream dev_null(nullptr);
    for(auto i = 0; i < FLAGS_iterations; i++){
        if(FLAGS_dump_size <= bigChmOfStrings.size()){
            bigChmOfStrings.clear();
        }
        // buggy, but aside the point.
        while(new_str.length() < 100){
            new_str.append(to<string>(folly::Random::rand32()));
        }
        if( UINT32_MAX * (FLAGS_mem_intensive_ratio/100) > folly::Random::rand32()){
            bigChmOfStrings.insert_or_assign(new_str, old_str);
            old_str = new_str;
            new_str.clear();
        } else {
            dev_null << old_str << new_str << endl;
        }
    }
    co_return;
}

Task<void> run() {
    folly::coro::AsyncScope backgroundScope;
    for(auto i = 0; i < FLAGS_threads; i++){
        backgroundScope.add(burn_cycles().scheduleOn(getGlobalCPUExecutor().get()));
    }
    cout << "Started all cycle/mem burning tasks." << endl;
    co_await backgroundScope.joinAsync();
    co_return;
}

int main(int argc, char* argv[]) {
    folly::init(&argc, &argv, true);
    auto start = system_clock::now().time_since_epoch();
    cout << "Run settings iterations: " << to<string>(FLAGS_iterations)
            << " threads: " << to<string>(FLAGS_threads)
            << " ratio_of_operations: " << to<string>(FLAGS_mem_intensive_ratio)
            << " map_dump_size: " << to<string>(FLAGS_dump_size)
            << endl;
    blockingWait(run());
    cout << "Finished all cycle/mem burning tasks." << endl;
    auto t_delta = system_clock::now().time_since_epoch()-start;
    auto duration = duration_cast<seconds>(t_delta).count();
    cout << "Took " << to<string>(duration) << " seconds." << endl;
    string k;
    string v;
    size_t s;
    k = bigChmOfStrings.begin()->first;
    v = bigChmOfStrings.begin()->second;
    s = bigChmOfStrings.size();
    cout << "Example of junk data (k v): (" << to<string>(k) << " " << to<string>(v) << ") " << endl;
    cout << "Junk data entries: " << to<string>(s) << endl;
    return 0;
}


