FROM ghcr.io/terrapkg/builder:f44 AS build
WORKDIR /build
RUN dnf install -y gcc git rustup /usr/lib/rpm/rpmdeps && dnf clean all
RUN rustup-init --default-toolchain stable --profile minimal -y -q
ENV PATH="/root/.cargo/bin:${PATH}"
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release --bins
# Derive runtime RPM dependencies from both dynamically linked executables.
RUN /usr/lib/rpm/rpmdeps --define="_use_internal_dependency_generator 1" \
      --requires target/release/rpmspecfmt target/release/rpmspeclint \
      > /build/deps.unsorted && sort -u /build/deps.unsorted > /build/deps.txt

FROM registry.fedoraproject.org/fedora-minimal:44
COPY --from=build /build/target/release/rpmspecfmt /usr/local/bin/rpmspecfmt
COPY --from=build /build/target/release/rpmspeclint /usr/local/bin/rpmspeclint
COPY --from=build /build/deps.txt /tmp/rpmspecfmt-deps.txt
RUN dnf install -y --setopt=install_weak_deps=0 $(cat /tmp/rpmspecfmt-deps.txt) \
    && dnf clean all && rm /tmp/rpmspecfmt-deps.txt
COPY LICENSE /usr/share/licenses/rpmspecfmt/LICENSE
WORKDIR /work
CMD ["rpmspecfmt"]
