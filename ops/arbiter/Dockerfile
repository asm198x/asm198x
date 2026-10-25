# The arbiter container: the reference assemblers this project measures itself
# against, one pinned version of each, normally the latest upstream release.
#
# `ops/arbiter/identities` lists the version line each tool must report, and
# `ops/arbiter/verify.sh` checks it. A pin and its line there change together:
# growth records which version answered each verdict and prunes any other
# (decisions/verdicts-track-current-tools.md).
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git subversion \
    && rm -rf /var/lib/apt/lists/*

# --- asl / p2bin ---------------------------------------------------------------
# The rolling 1.42 beta, published per build number. The upstream site keeps
# every build, so a build number pins exactly.
# The generic Linux definition rather than an arch-specific one: this image is
# built on both arm64 (a maintainer's machine) and amd64 (a CI runner), and the
# identity string carries no architecture — asl prints the triple on the line
# *after* the one the corpus keys on.
ARG ASL_BUILD=311
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

# --- acme --------------------------------------------------------------------
# No release tarballs and no tags: the upstream SVN trunk revision is the pin.
# Revision 448 (August 2025) reports `release 0.97 ("Zem"), 1 Aug 2025`.
ARG ACME_REV=448
RUN svn export -q -r "${ACME_REV}" \
        https://svn.code.sf.net/p/acme-crossass/code-0/trunk /tmp/acme \
    && make -C /tmp/acme/src -j"$(nproc)" \
    && install -m 0755 /tmp/acme/src/acme /usr/local/bin/ \
    && rm -rf /tmp/acme

# --- cc65 (ca65, ld65) --------------------------------------------------------
# cc65 has tagged no release since 2.19 (2020), so the pin is a commit on its
# default branch. Its tools put the build in their version line, as `V2.19 -
# Git <sha>`; BUILD_ID is passed explicitly so the line names the pin with a
# seven-character hash however git would abbreviate it in this clone.
ARG CC65_REV=8d86ca7e7ed15300cc4bfaba61a40a3d00f3114d
RUN mkdir -p /tmp/cc65 && cd /tmp/cc65 \
    && git init -q && git remote add origin https://github.com/cc65/cc65.git \
    && git fetch -q --depth 1 origin "${CC65_REV}" && git checkout -q FETCH_HEAD \
    && make -C /tmp/cc65 -j"$(nproc)" BUILD_ID="Git $(echo "${CC65_REV}" | cut -c1-7)" \
    && install -m 0755 /tmp/cc65/bin/ca65 /tmp/cc65/bin/ld65 /usr/local/bin/ \
    && rm -rf /tmp/cc65

# --- lwtools 4.25 ------------------------------------------------------------
RUN curl -fsSL https://www.lwtools.ca/releases/lwtools/lwtools-4.25.tar.gz -o /tmp/lw.tar.gz \
    && mkdir -p /tmp/lw && tar -xzf /tmp/lw.tar.gz -C /tmp/lw --strip-components=1 \
    && make -C /tmp/lw -j"$(nproc)" \
    && install -m 0755 /tmp/lw/lwasm/lwasm /usr/local/bin/ \
    && rm -rf /tmp/lw /tmp/lw.tar.gz

# --- rgbds -------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
        bison flex libpng-dev pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && git clone --depth 1 --branch v1.0.4 https://github.com/gbdev/rgbds.git /tmp/rgbds \
    && make -C /tmp/rgbds -j"$(nproc)" \
    && install -m 0755 /tmp/rgbds/rgbasm /tmp/rgbds/rgblink /tmp/rgbds/rgbfix /usr/local/bin/ \
    && rm -rf /tmp/rgbds

# --- sjasmplus ---------------------------------------------------------------
RUN git clone --depth 1 --recursive --branch v1.24.0 \
        https://github.com/z00m128/sjasmplus.git /tmp/sj \
    && make -C /tmp/sj -j"$(nproc)" \
    && install -m 0755 /tmp/sj/sjasmplus /usr/local/bin/ \
    && rm -rf /tmp/sj

# --- vasm (current release) --------------------------------------------------
# The one tool here with no pin available. Upstream serves a single
# `vasm.tar.gz` that is always the current release and keeps no versioned
# archives (2.0f, July 2026, when this was written).
#
# So this fetches current and lets `verify-arbiters` decide. When upstream
# releases again the identity check fails and the image will not build, which
# is the point: an upstream release becomes a reviewed bump rather than a
# silent one.
RUN curl -fsSL http://sun.hasenbraten.de/vasm/release/vasm.tar.gz -o /tmp/vasm.tar.gz \
    && mkdir -p /tmp/vasm && tar -xzf /tmp/vasm.tar.gz -C /tmp/vasm --strip-components=1 \
    && make -C /tmp/vasm CPU=m68k SYNTAX=mot -j"$(nproc)" \
    && install -m 0755 /tmp/vasm/vasmm68k_mot /usr/local/bin/ \
    && rm -rf /tmp/vasm /tmp/vasm.tar.gz

# --- PasmoNext ---------------------------------------------------------------
# A fork with no tags, so a commit is the pin; this is its latest.
ARG PASMO_REV=60957e69ed97eadf0f1546a8bfcd2c10a987f64a
RUN git clone -q https://github.com/Ckirby101/pasmoNext.git /tmp/pn \
    && git -C /tmp/pn checkout -q "${PASMO_REV}" \
    && cd /tmp/pn/pasmo && g++ -O2 -o pasmo *.cpp \
    && install -m 0755 pasmo /usr/local/bin/ \
    && rm -rf /tmp/pn

# --- Rust ---------------------------------------------------------------------
# Growth runs the ignored test suites, so this image needs the toolchain as well
# as the assemblers. The channel is read from the repository at build time
# rather than written twice: a growth run on a different compiler than CI is a
# difference nobody would think to look for.
ARG RUST_CHANNEL=1.95.0
RUN curl -fsSL https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --no-modify-path --profile minimal \
        --default-toolchain "${RUST_CHANNEL}" \
    && rm /tmp/rustup.sh
ENV PATH=/root/.cargo/bin:$PATH

# --- Python -------------------------------------------------------------------
# Some curriculum programs include a file their Makefile generates with a
# Python script, such as the Spectrum two-word parser's vocabulary. Growth
# runs the curriculum's own generator before arbitrating, so the image needs
# the interpreter.
RUN apt-get update && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /usr/local/share/asm198x
COPY identities /usr/local/share/asm198x/identities
COPY verify.sh /usr/local/bin/verify-arbiters
CMD ["/usr/local/bin/verify-arbiters"]
