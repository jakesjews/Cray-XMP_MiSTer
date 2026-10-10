# The cross-compilers for COS on the CRAY X-MP: Kevin Jordan's fork of the
# Amsterdam Compiler Kit, with the assembler, loader and library manager of
# his COS-Tools, which the kit calls.
#
#   docker build -t cray-ack tools/ack
#   docker run --rm -v "$PWD":/work cray-ack ack -mcos -o hello.abs hello.c
FROM debian:bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential git ca-certificates bison flex lua5.3 lua-posix \
 && rm -rf /var/lib/apt/lists/*
# The assembler moves the rest of a line up against a name that is too long
# with strcpy, whose two strings must not overlap.  With this C library the
# line comes out garbled, and the kit's own C library does not assemble
# (OPERAND FIELD ERROR in flushbuf).
RUN git clone https://github.com/kej715/COS-Tools /src/COS-Tools \
 && cd /src/COS-Tools && git checkout 6600331 \
 && sed -i 's/strcpy(start + MAX_NAME_LENGTH, s);/memmove(start + MAX_NAME_LENGTH, s, strlen(s) + 1);/' parse.c \
 && make cal dasm ldr lib && install -m 755 cal dasm ldr lib /usr/local/bin
RUN git clone https://github.com/kej715/ack /src/ack \
 && cd /src/ack && git checkout 03b8233 && ./cray-build.sh && ./cray-build.sh install \
 && rm -rf /tmp/ack-build
WORKDIR /work
