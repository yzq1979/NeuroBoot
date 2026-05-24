---
name: /diagnose-network
description: 用户说连不上网 / 网慢 / DNS 错时的诊断剧本
---

按以下顺序诊断网络问题：

## 1. 先看硬件层 (list_network_adapters)
判断：
- 网卡是否 Up（Disconnected = 物理断 / 驱动问题；Disabled = 用户禁了；NotPresent = 没装驱动）
- LinkSpeed 是否合理（千兆口应该 1Gbps；只有 100Mbps 可能线缆 / 网口 / 协商失败）
- DriverVersion 是否异常老

## 2. 再看 IP/DNS 层 (read_ip_config)
判断：
- IPv4Address 是否拿到（空 = DHCP 失败 / 网卡禁 / 没接线）
- IPv4Gateway 是否有 + 是否在同子网（不在子网 = 子网掩码配错）
- **DNSServers 重点检查**：
  - 标准公共 DNS：8.8.8.8 / 8.8.4.4 (Google) / 1.1.1.1 (Cloudflare) / 114.114.114.114 (114 DNS) / 223.5.5.5 (阿里)
  - 路由器内网 IP：常见 192.168.x.1
  - **可疑**：任何陌生公网 IP（恶意软件常改 DNS 劫持流量）

## 3. 服务层 (list_services 过滤几个关键服务)
- Dhcp（DHCP Client）= Running 才能拿到 IP
- Dnscache（DNS Client）= Running 才能解析域名
- LanmanWorkstation = 局域网共享访问
- IKEEXT / RasMan = VPN 依赖

## 4. PE 注意
PE 默认**不自动连 Wi-Fi**。用户要在 PE 里上网必须：
- 有线网线插上（DHCP 通常自动配）
- 或者敲 `wpeutil InitializeNetwork` 启用网络后用 `netsh wlan connect` 手动连 Wi-Fi
- 或者把所需的诊断在主系统跑

## 5. 报告格式
```
网卡硬件：✅/⚠/❌
IP 配置：✅/⚠/❌
DNS 配置：✅/⚠/❌（如有可疑 DNS 标红）
诊断结论：[一句话]
建议：[1~3 个具体操作]
```
