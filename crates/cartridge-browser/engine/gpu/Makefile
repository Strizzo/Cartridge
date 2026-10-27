CC ?= cc
CFLAGS ?= -O2
WARNINGS = -std=c11 -Wall -Wextra -Werror -fPIC -fvisibility=hidden
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
SHARED = -dynamiclib
SONAME = -Wl,-install_name,@rpath/libgbm.so.1
LDLIBS = -lpthread
else
SHARED = -shared
SONAME = -Wl,-soname,libgbm.so.1 -Wl,-Bsymbolic-functions -Wl,-z,defs -Wl,-z,relro,-z,now
LDLIBS = -ldl -lpthread
endif

.PHONY: all test clean
all: build/libgbm.so.1
build/libgbm.so.1: gbm_guard.c gbm_api.h
	mkdir -p build
	$(CC) $(CFLAGS) $(WARNINGS) $(SHARED) $(SONAME) gbm_guard.c -o $@ $(LDLIBS)
test:
	python3 -m unittest discover -s tests -v
clean:
	rm -rf build
