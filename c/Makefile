CC       := gcc
CFLAGS   := -std=c11 -Wall -Wextra -pedantic -O2
PROTO    := ../proto/diff.proto
PROTO_C  := diff.pb-c

# Detect protobuf-c via pkg-config, fall back to plain -lprotobuf-c
PBCFLAGS := $(shell pkg-config --cflags protobuf-c 2>/dev/null)
PBCLIBS  := $(shell pkg-config --libs   protobuf-c 2>/dev/null || echo -lprotobuf-c)

SRCS     := diff_apply.c $(PROTO_C).c
OBJS     := $(SRCS:.c=.o)
LIB      := libdiff_apply.a
TEST_BIN := test_diff_apply

.PHONY: all proto test clean

all: proto $(LIB)

# Generate C sources from the proto file
proto: $(PROTO_C).c $(PROTO_C).h

$(PROTO_C).c $(PROTO_C).h: $(PROTO)
	protoc-c --c_out=. --proto_path=$(dir $(PROTO)) $(notdir $(PROTO))

# Compile object files
%.o: %.c
	$(CC) $(CFLAGS) $(PBCFLAGS) -c $< -o $@

# Static library
$(LIB): $(OBJS)
	$(AR) rcs $@ $^

# Test binary — links test runner against the library
$(TEST_BIN): proto test_diff_apply.o $(LIB)
	$(CC) $(CFLAGS) $(PBCFLAGS) test_diff_apply.o $(LIB) $(PBCLIBS) -o $@

test: $(TEST_BIN)
	./$(TEST_BIN)

# Windows (MSVC) — no make/gcc available:
#   powershell -ExecutionPolicy Bypass -File build_test.ps1

clean:
	$(RM) $(OBJS) test_diff_apply.o $(LIB) $(TEST_BIN) $(PROTO_C).c $(PROTO_C).h
