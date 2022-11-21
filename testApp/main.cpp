#include <iostream>
#include <folly/container/F14Map.h>
#include <folly/init/Init.h>
#include <folly/experimental/coro/Task.h>
#include <folly/experimental/coro/BlockingWait.h>
#include <folly/experimental/coro/AsyncScope.h>
#include <gflags/gflags.h>
#include <xoshiro-cpp/XoshiroCpp.hpp>
#include <folly/Conv.h>
#include <glog/logging.h>
#include <chrono>
#include <folly/Synchronized.h>
#include <folly/concurrency/ConcurrentHashMap.h>

using namespace std;
using namespace folly;
using namespace coro;
using namespace XoshiroCpp;
using namespace chrono;

Synchronized<F14FastMap<string,string>> bigMapOfStrings;
ConcurrentHashMap<string, string> bigChmOfStrings;

DEFINE_uint64(threads, 1, "Threads to waste (effectively).");
DEFINE_uint64(iterations, 100, "How much pointless stuff to do.");
DEFINE_bool(mem_intensive, false, "Do mem intensive stuff.");
DEFINE_bool(use_chm, false, "Use chm for mem intensive stuff.");

Task<void> burn_cycles() {
    const std::uint64_t seed = 12345;
    SplitMix64 rng(seed);
    string new_str;
    string old_str;
    std::ostream dev_null(0);
    for(auto i = 0; i < FLAGS_iterations; i++){
        // buggy, but aside the point.
        while(new_str.length() < 100){
            uint64_t data = rng();
            new_str.append(to<string>(data));
        }
        if(FLAGS_mem_intensive){
            if(FLAGS_use_chm){
                bigChmOfStrings.insert_or_assign(new_str, old_str);
            } else {
                auto locked = bigMapOfStrings.wlock();
                locked->insert_or_assign(new_str, old_str);
                locked.unlock();
            }
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
            << " mem_intensive: " << to<string>(FLAGS_mem_intensive)
            << " use_chm: " << to<string>(FLAGS_use_chm)
            << endl;
    blockingWait(run());
    cout << "Finished all cycle/mem burning tasks." << endl;
    auto t_delta = system_clock::now().time_since_epoch()-start;
    auto duration = duration_cast<seconds>(t_delta).count();
    cout << "Took " << to<string>(duration) << " seconds." << endl;
    if(FLAGS_mem_intensive){
        string k;
        string v;
        long s;
        if(FLAGS_use_chm){
            k = bigChmOfStrings.begin()->first;
            v = bigChmOfStrings.begin()->second;
            s = bigChmOfStrings.size();
        } else {
            auto locked = bigMapOfStrings.rlock();
            k = locked->begin()->first;
            v = locked->begin()->second;
            s = locked->size();
            locked.unlock();
        }
        cout << "Example of junk data (k v): (" << to<string>(k) << " " << to<string>(v) << ") " << endl;
        cout << "Junk data entries: " << to<string>(s) << endl;
    }
    return 0;
}


