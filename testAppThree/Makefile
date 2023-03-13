CC      = gcc
CFLAGS  = -g -mtune=native -march=native -pipe -fno-omit-frame-pointer -gstrict-dwarf -gdwarf-4 -O0
RM      = rm -f

default: all

all: verOne verTwo

verOne: verOne.c
	$(CC) $(CFLAGS) -o verOne verOne.c

verTwo: verTwo.c
	$(CC) $(CFLAGS) -o verTwo verTwo.c

clean:
	$(RM) verOne
	$(RM) verTwo
