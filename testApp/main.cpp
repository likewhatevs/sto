#include <main.h>

using namespace std;
using namespace folly;
using namespace std::chrono;
using namespace coro;

folly::coro::CancellableAsyncScope backgroundScope;
ConcurrentHashMapSIMD<string, string> bigMapOfStr;
folly::coro::Mutex map_lock;

DEFINE_uint64(threads, 10, "Threads to waste (effectively).");
DEFINE_uint64(seconds, 15, "seconds to waste compute for");
DEFINE_double(mem_intensive_ratio, 0.5, "Do mem intensive stuff X % of time, 0-1.");
DEFINE_uint64(dump_size, 1000, "What capacity to clear map at.");

Task<void> map_write(std::string k, std::string v){
    co_await map_lock.co_scoped_lock();
    bigMapOfStr.insert_or_assign(k, v);
    co_return;
}

Task<void> map_clear(){
    co_await map_lock.co_scoped_lock();
    bigMapOfStr.clear();
    co_return;
}

Task<uint64_t> map_size(){
    co_await map_lock.co_scoped_lock();
    co_return bigMapOfStr.size();
}

Task<void> burn_cycles(auto start) {
    string new_str;
    string old_str;
    std::ostream dev_null(nullptr);
    auto cur = std::chrono::system_clock::now();
    while(cur < start + std::chrono::seconds(FLAGS_seconds)){
        auto items = co_await map_size();
        if(FLAGS_dump_size <= 0){
            XLOG(INFO) << "clearing map";
            co_await map_clear();
        }
        while(new_str.length() < 100){
            new_str.append(to<string>(folly::Random::rand32()));
        }
        if(folly::Random::randDouble(0, 1.0)>FLAGS_mem_intensive_ratio){
            co_await map_write(old_str, new_str);
            old_str = new_str;
            new_str.clear();
        } else {
            dev_null << old_str << new_str << endl;
        }
    }
    co_return;
}

Task<void> run() {
    auto start = std::chrono::system_clock::now();
    for(auto i = 0; i < FLAGS_threads; i++){
        backgroundScope.add(burn_cycles(start).scheduleOn(getGlobalCPUExecutor().get()));
        XLOG(DBG) << "Reassigned bs tasks." << endl;
    }
    XLOG(DBG) << "Started tasks." << endl;
    if(!backgroundScope.isScopeCancellationRequested()) {
        co_await backgroundScope.joinAsync();
    }
    XLOG(DBG) << "Awaited tasks." << endl;
    co_return;
}

void signal_callback_handler(int signum) {
    if(!backgroundScope.isScopeCancellationRequested()){
        blockingWait(backgroundScope.cancelAndJoinAsync());
    }
    exit(0);
}

int main(int argc, char* argv[]) {
    folly::init(&argc, &argv, true);
    signal(SIGINT, signal_callback_handler);
    signal(SIGTERM, signal_callback_handler);
    signal(SIGABRT, signal_callback_handler);
    signal(SIGKILL, signal_callback_handler);
    XLOG(INFO) << "PID: " << getpid();
    XLOG(INFO) << "Run settings seconds: " << to<string>(FLAGS_seconds)
         << " threads: " << to<string>(FLAGS_threads)
         << " ratio_of_operations: " << to<string>(FLAGS_mem_intensive_ratio)
         << " map_dump_size: " << to<string>(FLAGS_dump_size)
         << endl;
    XLOG(INFO) << "PID: " << getpid();
    blockingWait(run());
    exit(0);
}


