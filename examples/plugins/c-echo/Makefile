# Build the c-echo loadr protocol plugin (C-ABI).
#
#   make            # libloadr_plugin_cecho.so  (Linux)
#   make clean
#
# macOS:   cc -dynamiclib -fPIC -o libloadr_plugin_cecho.dylib cecho.c
# Windows: cl /LD /Fe:loadr_plugin_cecho.dll cecho.c

CC      ?= cc
CFLAGS  ?= -O2 -fPIC -Wall -Wextra
UNAME_S := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
  LIB     := libloadr_plugin_cecho.dylib
  LDFLAGS := -dynamiclib
else
  LIB     := libloadr_plugin_cecho.so
  LDFLAGS := -shared
endif

$(LIB): cecho.c
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $<

.PHONY: clean
clean:
	rm -f libloadr_plugin_cecho.so libloadr_plugin_cecho.dylib loadr_plugin_cecho.dll
