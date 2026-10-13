FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends openssh-server \
 && rm -rf /var/lib/apt/lists/*
# 测试用户：密码含 + _ 等特殊字符
RUN useradd -m -s /bin/bash tuser \
 && echo 'tuser:Pa+ss_w0rd' | chpasswd
RUN mkdir -p /run/sshd /etc/ssh/hostkeys
COPY sshd_config /etc/ssh/sshd_config
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
