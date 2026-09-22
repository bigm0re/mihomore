mihomore —— mihomo 可视化管理器

【这是什么】
一个 Windows 桌面程序，用于管理 mihomo 内核：仪表盘、订阅、代理、连接、规则、覆写与设置。
无需安装，解压即用。

【使用方法】
1. 把 mihomore.exe 放到任意你有写权限的目录（推荐单独建一个文件夹）。
2. 双击运行。首次运行会自动在同目录生成：
     config\   配置与订阅（app.config.toml、core.common.config.yaml、subscriptions\）
     data\     覆写脚本与日志（override.js、logs\mihomore.log、logs\core.log）
     cache\    内核与地理数据（core\mihomo.exe、geoip.dat 等）
   所有数据都跟着 exe 走，拷贝整个文件夹即可迁移或备份。
3. 进入「仪表盘」，点右下角「启动内核」；开关系统代理、虚拟网卡（TUN）、出站模式都在这一页。

【仪表盘】
- 网络速度：实时上/下行曲线。
- 流量统计：环形图与本次会话累计上传/下载。
- 系统代理：与 Windows「设置 → 网络和 Internet → 代理」保持双向一致；
  关闭时只回收由本程序开启的代理，不会影响你自己配置的代理。
- 出站模式：规则 / 全局 / 直连。
- 网络检测：内网 IP 与公网 IPv4/IPv6，可点刷新重新检测。
- 右下角悬浮按钮：显示内核运行时长，再次点击可停止内核。

窗口缩放时仪表盘会自动重排卡片；空间不足时页面出现滚动条，右下角按钮始终可见。

【便携模式】
默认把配置放在 exe 同级目录。如需指定其它目录，可设置环境变量：
    set MIHOMORE_HOME=D:\mihomore-data
若软件目录不可写（例如放在 Program Files），会自动回退到系统用户目录。

【日志排查】
应用日志在 data\logs\mihomore.log，内核日志在 data\logs\core.log。
想临时在终端里同时看日志：先执行  set MIHOMORE_LOG_STDERR=1  再从终端启动。

【注意】
- 开启「虚拟网卡」需要管理员权限；若提示需要内核服务，先在设置页安装内核服务。
- 关闭应用时会自动停掉内核，并恢复你原来的系统代理设置（只回收由本程序开启的代理）。
