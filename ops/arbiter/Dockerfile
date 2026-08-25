# The arbiter container: the eight reference assemblers this project measures
# itself against, pinned to the exact versions the verdict corpus records.
#
# A verdict is keyed on the arbiter's *behavioural identity* — its own version
# self-report — so this image is only useful if it reproduces those strings
# exactly. A different binary is fine and expected: the corpus records a binary
# digest as provenance so a second build corroborates a verdict rather than
# forking it. A different identity string is not fine; it would key every
# verdict this container records separately from every verdict already held.
#
# `ops/arbiter/identities` is the list to match. `ops/arbiter/verify.sh` checks.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git \
    && rm -rf /var/lib/apt/lists/*

# --- asl / p2bin — 45% of the corpus -----------------------------------------
# The rolling 1.42 beta, published per build number. `Bld 309` is what the
# corpus records, and the upstream site keeps every build, so it pins exactly.
# The generic Linux definition rather than an arch-specific one: this image is
# built on both arm64 (a maintainer's machine) and amd64 (a CI runner), and the
# identity string carries no architecture — asl prints the triple on the line
# *after* the one the corpus keys on.
ARG ASL_BUILD=309
RUN curl -fsSL "http://john.ccac.rwth-aachen.de:8000/ftp/as/source/c_version/asl-current-142-bld${ASL_BUILD}.tar.gz" \
      -o /tmp/asl.tar.gz \
    && mkdir -p /tmp/asl && tar -xzf /tmp/asl.tar.gz -C /tmp/asl --strip-components=1 \
    && cd /tmp/asl \
    && cp Makefile.def-samples/Makefile.def-unknown-linux Makefile.def \
    && make -j"$(nproc)" \
    && install -m 0755 asl p2bin /usr/local/bin/ \
    && mkdir -p /usr/local/share/asl && cp -r include /usr/local/share/asl/ \
    && rm -rf /tmp/asl /tmp/asl.tar.gz
ENV AS_INCLUDE=/usr/local/share/asl/include

# --- acme 0.97 ---------------------------------------------------------------
RUN git clone --depth 1 --branch r0.97 https://github.com/meonwax/acme.git /tmp/acme \
    && make -C /tmp/acme/src -j"$(nproc)" \
    && install -m 0755 /tmp/acme/src/acme /usr/local/bin/ \
    && rm -rf /tmp/acme

# --- cc65 2.19 — its ca65 reports V2.18, which is what the corpus holds -------
RUN git clone --depth 1 --branch V2.19 https://github.com/cc65/cc65.git /tmp/cc65 \
    && make -C /tmp/cc65 -j"$(nproc)" \
    && install -m 0755 /tmp/cc65/bin/ca65 /tmp/cc65/bin/ld65 /usr/local/bin/ \
    && rm -rf /tmp/cc65

# --- lwtools 4.25 ------------------------------------------------------------
RUN curl -fsSL http://www.lwtools.ca/releases/lwtools/lwtools-4.25.tar.gz -o /tmp/lw.tar.gz \
    && mkdir -p /tmp/lw && tar -xzf /tmp/lw.tar.gz -C /tmp/lw --strip-components=1 \
    && make -C /tmp/lw -j"$(nproc)" \
    && install -m 0755 /tmp/lw/lwasm/lwasm /usr/local/bin/ \
    && rm -rf /tmp/lw /tmp/lw.tar.gz

# --- rgbds 1.0.3 -------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
        bison flex libpng-dev pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && git clone --depth 1 --branch v1.0.3 https://github.com/gbdev/rgbds.git /tmp/rgbds \
    && make -C /tmp/rgbds -j"$(nproc)" \
    && install -m 0755 /tmp/rgbds/rgbasm /tmp/rgbds/rgblink /usr/local/bin/ \
    && rm -rf /tmp/rgbds

# --- sjasmplus 1.21.0 --------------------------------------------------------
RUN git clone --depth 1 --recursive --branch v1.21.0 \
        https://github.com/z00m128/sjasmplus.git /tmp/sj \
    && make -C /tmp/sj -j"$(nproc)" \
    && install -m 0755 /tmp/sj/sjasmplus /usr/local/bin/ \
    && rm -rf /tmp/sj

# --- vasm 2.0b (Motorola syntax, m68k) ---------------------------------------
RUN curl -fsSL http://sun.hasenbraten.de/vasm/release/vasm.tar.gz -o /tmp/vasm.tar.gz \
    && mkdir -p /tmp/vasm && tar -xzf /tmp/vasm.tar.gz -C /tmp/vasm --strip-components=1 \
    && make -C /tmp/vasm CPU=m68k SYNTAX=mot -j"$(nproc)" \
    && install -m 0755 /tmp/vasm/vasmm68k_mot /usr/local/bin/ \
    && rm -rf /tmp/vasm /tmp/vasm.tar.gz

# --- PasmoNext ---------------------------------------------------------------
# A fork, so it pins to its own repository rather than a release.
RUN git clone --depth 1 https://github.com/Ckirby101/pasmoNext.git /tmp/pn \
    && cd /tmp/pn/pasmo && g++ -O2 -o pasmo *.cpp \
    && install -m 0755 pasmo /usr/local/bin/ \
    && rm -rf /tmp/pn

RUN mkdir -p /usr/local/share/asm198x
COPY identities /usr/local/share/asm198x/identities
COPY verify.sh /usr/local/bin/verify-arbiters
CMD ["/usr/local/bin/verify-arbiters"]
