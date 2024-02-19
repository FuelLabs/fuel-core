FROM ubuntu:20.04

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    apt-get -y install -y \
    ca-certificates libssl1.1 vim htop iotop sysstat \
    dstat strace lsof curl jq tzdata && \
    rm -rf /var/cache/apt /var/lib/apt/lists/*

RUN rm /etc/localtime && ln -snf /usr/share/zoneinfo/America/Montreal /etc/localtime && dpkg-reconfigure -f noninteractive tzdata
RUN mkdir -p /app/ && curl -Lo /app/grpc_health_probe https://github.com/grpc-ecosystem/grpc-health-probe/releases/download/v0.4.12/grpc_health_probe-linux-amd64 && chmod +x /app/grpc_health_probe

ADD /firefuel /app/firefuel

COPY tools/docker/motd_generic /etc/motd
COPY tools/docker/99-firehose.sh /etc/profile.d/

# On SSH connection, /root/.bashrc is invoked which invokes '/root/.bash_aliases' if it exists,
# so we hijack the file to "execute" our specialized bash script
RUN echo ". /etc/profile.d/99-firehose.sh" > /root/.bash_aliases

ENV PATH "$PATH:/app"

ENTRYPOINT ["/app/firefuel"]
