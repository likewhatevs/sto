CC      = gcc
CFLAGS  = -g -mtune=native -march=native -pipe -fno-omit-frame-pointer -gstrict-dwarf -gdwarf-4 -O0
RM      = rm -f

default: all

all: demo

demo: demo.c
	$(CC) $(CFLAGS) -o demo demo.c

clean:
	$(RM) demo