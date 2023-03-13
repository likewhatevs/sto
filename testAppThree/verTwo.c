#include <stdio.h>
#include <unistd.h>

void awesomeDependency(){
    int i = 100000*300;
    while (i > 0) {
        int a = 2;
        int b = a * 2;
        i = i - 1 ;
    }
}

void doMathStuff(){
    awesomeDependency();
    awesomeDependency();
}

void doBusinessLogic(){
    int i = 100000*100;
    while (i > 0) {
        int a = 2;
        int b = a * 2;
        i = i - 1 ;
    }
}

int main()
{
    printf("%ld\n", (long)getpid());
    while(1){
        doMathStuff();
        doBusinessLogic();
        awesomeDependency();
    }
}
