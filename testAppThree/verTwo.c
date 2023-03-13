#include <stdio.h>
#include <unistd.h>

void awesomeDependency(){
    int i = 1000000000;
    while (i > 0) {
        int a = 2;
        int b = a * 2;
        i = i - 1 ;
    }
    sleep(1);
}

void doMathStuff(){
    awesomeDependency();
}

void doBusinessLogic(){
    sleep(1);
}

int main()
{
    printf("%ld\n", (long)getpid());
    while(1){
        doMathStuff();
        doBusinessLogic();
    }
}
