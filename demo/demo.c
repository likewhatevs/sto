#include <stdio.h>
#include <unistd.h>
#include <stdlib.h>

void simulateWork(int x) { for(int i = 1000000*x; i>0; i--); }

void doBusinessLogic(int x) { simulateWork(x); }

void doLogging(int x) { simulateWork(x); }

void applicationLogic(int logPct) {
    if(rand() % 100 < logPct) {
        doBusinessLogic(1);
        doLogging(10);
    } else {
        doBusinessLogic(1);
    }
}

int main() {
    printf("%ld\n", (long)getpid());
    while(1){ applicationLogic(50); }
}

