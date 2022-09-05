#!/usr/bin/env sh

export DEBIAN_FRONTEND=noninteractive

# Install and configure LLD for faster linking

apt-get update
apt-get install -y --no-install-recommends lld

cat > /usr/local/cargo/config.toml <<-EOF
    [target.x86_64-unknown-linux-gnu]
    rustflags = ["-C", "link-arg=-fuse-ld=lld"]
EOF

# Install Protobuf and Thrift compilers

echo 'deb http://deb.debian.org/debian bookworm main' >> /etc/apt/sources.list
cat > /etc/apt/preferences.d/testing <<-EOF
    Package: *
    Pin: release a=testing
    Pin-Priority: 100
EOF

apt-get update
apt-get -y install --no-install-recommends -t testing \
    libprotobuf-dev \
    protobuf-compiler \
    thrift-compiler
