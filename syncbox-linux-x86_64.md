# Linux 二进制文件使用指南

1. 直接下载压缩包到当前目录，文件名保持清晰 \
`curl -L https://github.com/ligang-code2020/syncbox/releases/download/v0.1.0/syncbox-linux-x86_64.tar.gz -o syncbox-linux.tar.gz`


2. `tar -zxvf syncbox-linux.tar.gz`


3. 赋予执行权限（若解压后文件在当前目录）\
`chmod +x syncbox`


4. 移动到 /usr/local/bin 目录（全局可用，需 sudo 权限）\
`sudo mv syncbox /usr/local/bin/`


5. 查看全局帮助 \
`syncbox --help`


6. 查看 sync 子命令的详细用法（核心同步功能）\
`syncbox sync --help`