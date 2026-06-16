# Build the go-echo loadr protocol plugin (C-ABI) via cgo's c-shared mode.
#
#   make            # libloadr_plugin_goecho.so   (Linux)
#   make clean
#
# c-shared needs cgo (a C toolchain on PATH). macOS emits a .dylib and Windows
# a .dll automatically based on the -o extension / GOOS.

CGO_ENABLED ?= 1
UNAME_S     := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
  LIB := libloadr_plugin_goecho.dylib
else
  LIB := libloadr_plugin_goecho.so
endif

$(LIB): goecho.go go.mod
	CGO_ENABLED=$(CGO_ENABLED) go build -buildmode=c-shared -o $@ .

.PHONY: clean
clean:
	rm -f libloadr_plugin_goecho.so libloadr_plugin_goecho.dylib loadr_plugin_goecho.dll libloadr_plugin_goecho.h
